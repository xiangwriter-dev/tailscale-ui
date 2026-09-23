use crate::{identity::cwd_in_roots, process::ProcessTree};
use sqlx::Row;
use std::{
    collections::HashMap,
    path::PathBuf,
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc,
    },
    time::Duration,
};
use tailtask_core::remote::*;
use tokio::{
    io::{AsyncRead, AsyncReadExt},
    sync::{mpsc, Mutex, Notify},
    task::JoinSet,
};
use tokio_util::sync::CancellationToken;

#[derive(Clone)]
pub struct Runner {
    pub store: AgentStore,
    pub accepting: Arc<AtomicBool>,
    pub admission: Arc<Mutex<()>>,
    pub notify: Arc<Notify>,
    pub stop: CancellationToken,
    active: Arc<Mutex<HashMap<String, CancellationToken>>>,
    roots: Vec<PathBuf>,
    directory: PathBuf,
    executable: PathBuf,
    concurrency: usize,
    pub fault: Arc<Mutex<Option<String>>>,
}
impl Runner {
    pub fn new(
        store: AgentStore,
        roots: Vec<PathBuf>,
        directory: PathBuf,
        executable: PathBuf,
        concurrency: u8,
    ) -> Self {
        Self {
            store,
            accepting: Arc::new(AtomicBool::new(true)),
            admission: Default::default(),
            notify: Arc::new(Notify::new()),
            stop: CancellationToken::new(),
            active: Default::default(),
            roots,
            directory,
            executable,
            concurrency: concurrency.clamp(1, 4) as usize,
            fault: Default::default(),
        }
    }
    pub async fn cancel(&self, id: &str, controller: Option<&str>) -> Result<RemoteTask, String> {
        let task = self.store.get(id, controller).await?;
        if task.state.terminal() {
            return Ok(task);
        }
        if !self
            .store
            .transition(
                id,
                &[RemoteState::Queued],
                RemoteState::Cancelled,
                None,
                "排队任务已取消",
            )
            .await?
        {
            self.store
                .transition(
                    id,
                    &[RemoteState::Starting, RemoteState::Running],
                    RemoteState::Cancelling,
                    None,
                    "正在取消执行实例",
                )
                .await?;
            if let Some(token) = self.active.lock().await.get(id) {
                token.cancel();
            }
        }
        self.store.get(id, controller).await
    }
    pub async fn request_stop(&self, abort: bool) -> Result<(), String> {
        let _guard = self.admission.lock().await;
        self.accepting.store(false, Ordering::SeqCst);
        if abort {
            let ids:Vec<String>=sqlx::query_scalar("SELECT id FROM agent_tasks WHERE state IN ('queued','starting','running','cancelling')").fetch_all(self.store.pool()).await.map_err(|e|e.to_string())?;
            for id in ids {
                self.cancel(&id, None).await?;
            }
        }
        self.stop.cancel();
        self.notify.notify_one();
        Ok(())
    }
    pub async fn run(self) -> Result<(), String> {
        let mut running = JoinSet::new();
        loop {
            while running.len() < self.concurrency && self.fault.lock().await.is_none() {
                let Some(task) = self.store.claim_next().await? else {
                    break;
                };
                let token = CancellationToken::new();
                self.active
                    .lock()
                    .await
                    .insert(task.id.clone(), token.clone());
                let this = self.clone();
                running.spawn(async move {
                    let id = task.id.clone();
                    let result = this.execute(task, token).await;
                    this.active.lock().await.remove(&id);
                    (id, result)
                });
            }
            if running.is_empty() && self.stop.is_cancelled() {
                return Ok(());
            }
            tokio::select! {
                result=running.join_next(),if !running.is_empty()=>{
                    match result {
                        Some(Ok((id,Err(error))))=>{self.accepting.store(false,Ordering::SeqCst);*self.fault.lock().await=Some(format!("RESULT_NOT_CONFIRMED: {id}: {error}"));},
                        Some(Err(error))=>{self.accepting.store(false,Ordering::SeqCst);*self.fault.lock().await=Some(format!("RUNNER_FAULT: {error}"));},
                        _=>{}
                    }
                },
                _=self.notify.notified()=>{},
                _=tokio::time::sleep(Duration::from_millis(500))=>{}
            }
        }
    }
    async fn execute(&self, task: RemoteTask, cancel: CancellationToken) -> Result<(), String> {
        let id = &task.id;
        // A cancellation may arrive after persistent claim but before registration.
        if self.store.get(id, None).await?.state == RemoteState::Cancelling {
            self.store
                .transition(
                    id,
                    &[RemoteState::Cancelling],
                    RemoteState::Cancelled,
                    None,
                    "启动前已取消",
                )
                .await?;
            return Ok(());
        }
        let started = self.start(&task).await;
        let mut process = match started {
            Ok(p) => p,
            Err(error) => {
                self.store
                    .transition(
                        id,
                        &[RemoteState::Starting, RemoteState::Cancelling],
                        RemoteState::Failed,
                        None,
                        &error,
                    )
                    .await?;
                return Ok(());
            }
        };
        self.store
            .transition(
                id,
                &[RemoteState::Starting],
                RemoteState::Running,
                None,
                "进程已启动",
            )
            .await?;
        let (sender, mut receiver) = mpsc::channel::<(String, String)>(64);
        let out = tokio::spawn(read_pipe(
            process.child.stdout.take().ok_or("STDOUT_MISSING")?,
            "stdout",
            sender.clone(),
        ));
        let err = tokio::spawn(read_pipe(
            process.child.stderr.take().ok_or("STDERR_MISSING")?,
            "stderr",
            sender.clone(),
        ));
        drop(sender);
        let deadline = tokio::time::sleep(Duration::from_secs(task.request.timeout_seconds as u64));
        tokio::pin!(deadline);
        let mut reason = None;
        let mut forced = false;
        let mut grace = Box::pin(tokio::time::sleep(Duration::from_secs(86401)));
        let status = loop {
            tokio::select! {biased;
                result=process.child.wait()=>{break result.map_err(|e|format!("PROCESS_WAIT: {e}"))?;},
                _=cancel.cancelled(),if reason.is_none()=>{reason=Some(RemoteState::Cancelled);process.graceful_stop();grace=Box::pin(tokio::time::sleep(Duration::from_secs(5)));},
                _=&mut deadline,if reason.is_none()=>{reason=Some(RemoteState::TimedOut);self.store.transition(id,&[RemoteState::Running,RemoteState::Starting],RemoteState::Cancelling,None,"任务超时，正在结束进程").await?;process.graceful_stop();grace=Box::pin(tokio::time::sleep(Duration::from_secs(5)));},
                _=&mut grace,if reason.is_some() && !forced=>{process.force_stop()?;forced=true;},
                Some((kind,text))=receiver.recv()=>{self.output(id,&kind,&text).await?;}
            }
        };
        // Terminate descendants retaining output handles even after the foreground
        // process has returned. We own this live Job/group, never an arbitrary PID.
        process.force_stop()?;
        tokio::time::timeout(Duration::from_secs(5), async {
            while let Some((kind, text)) = receiver.recv().await {
                self.output(id, &kind, &text).await?;
            }
            Ok::<(), String>(())
        })
        .await
        .map_err(|_| "PROCESS_OUTPUT_NOT_CLOSED: 子进程输出仍未关闭，结果待核实")??;
        out.await.map_err(|e| e.to_string())??;
        err.await.map_err(|e| e.to_string())??;
        let final_state = reason.unwrap_or(if status.success() {
            RemoteState::Succeeded
        } else {
            RemoteState::Failed
        });
        let message = match final_state {
            RemoteState::Succeeded => "进程正常退出",
            RemoteState::Cancelled => "执行实例已取消",
            RemoteState::TimedOut => "执行实例因超时结束",
            _ => "进程以非零状态退出",
        };
        self.store
            .transition(
                id,
                &[
                    RemoteState::Starting,
                    RemoteState::Running,
                    RemoteState::Cancelling,
                ],
                final_state,
                status.code(),
                message,
            )
            .await?;
        crate::artifacts::collect(&self.store, &task, &self.directory, &self.roots).await?;
        Ok(())
    }
    async fn start(&self, task: &RemoteTask) -> Result<ProcessTree, String> {
        cwd_in_roots(&task.request.cwd, &self.roots).await?;
        let work = self.directory.join("runs").join(&task.id);
        tokio::fs::create_dir_all(&work)
            .await
            .map_err(|e| e.to_string())?;
        ProcessTree::spawn(&self.executable, &task.request, work).await
    }
    async fn output(&self, id: &str, kind: &str, text: &str) -> Result<(), String> {
        let mut tx = self
            .store
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| e.to_string())?;
        let row = sqlx::query("SELECT output_bytes,output_truncated FROM agent_tasks WHERE id=?")
            .bind(id)
            .fetch_one(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        let retained = row.get::<i64, _>("output_bytes") as usize;
        if retained + text.len() > MAX_OUTPUT_BYTES {
            if row.get::<i64, _>("output_truncated") == 0 {
                insert_event(
                    &mut tx,
                    id,
                    "output_truncated",
                    "日志超过 16 MiB，后续输出不再保留；任务继续运行",
                )
                .await?;
                sqlx::query("UPDATE agent_tasks SET output_truncated=1 WHERE id=?")
                    .bind(id)
                    .execute(&mut *tx)
                    .await
                    .map_err(|e| e.to_string())?;
            }
        } else {
            sqlx::query("UPDATE agent_tasks SET output_bytes=output_bytes+? WHERE id=?")
                .bind(text.len() as i64)
                .bind(id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            insert_event(&mut tx, id, kind, text).await?;
            if kind == "stdout" {
                if let Some(progress) = reported_progress(text) {
                    sqlx::query("UPDATE agent_tasks SET progress=? WHERE id=?")
                        .bind(progress)
                        .bind(id)
                        .execute(&mut *tx)
                        .await
                        .map_err(|e| e.to_string())?;
                }
            }
        }
        tx.commit().await.map_err(|e| e.to_string())
    }
}

async fn insert_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Sqlite>,
    id: &str,
    kind: &str,
    text: &str,
) -> Result<(), String> {
    sqlx::query("INSERT INTO agent_events(task_id,seq,kind,text,occurred_at) SELECT ?,COALESCE(MAX(seq),0)+1,?,?,? FROM agent_events WHERE task_id=?").bind(id).bind(kind).bind(text).bind(tailtask_core::now()).bind(id).execute(&mut **tx).await.map_err(|e|e.to_string())?;
    Ok(())
}

pub fn reported_progress(line: &str) -> Option<f64> {
    let json = line
        .trim_end_matches(['\n', '\r'])
        .strip_prefix("TAILTASK_PROGRESS ")?;
    let value: serde_json::Value = serde_json::from_str(json).ok()?;
    let percent = value.get("percent")?.as_f64()?;
    if percent.is_finite() && (0.0..=100.0).contains(&percent) {
        Some(percent)
    } else {
        None
    }
}

async fn read_pipe<R: AsyncRead + Unpin>(
    mut pipe: R,
    kind: &str,
    sender: mpsc::Sender<(String, String)>,
) -> Result<(), String> {
    let mut input = [0u8; 4096];
    let mut pending = Vec::new();
    let mut line = String::new();
    let mut escape = 0u8;
    loop {
        let count = pipe
            .read(&mut input)
            .await
            .map_err(|e| format!("PIPE_READ: {e}"))?;
        pending.extend_from_slice(&input[..count]);
        let mut used = 0;
        while used < pending.len() {
            let (valid, invalid) = match std::str::from_utf8(&pending[used..]) {
                Ok(s) => (s.len(), 0),
                Err(e) => (
                    e.valid_up_to(),
                    e.error_len().unwrap_or(if count == 0 {
                        pending.len() - used - e.valid_up_to()
                    } else {
                        0
                    }),
                ),
            };
            let text =
                std::str::from_utf8(&pending[used..used + valid]).map_err(|e| e.to_string())?;
            for ch in text
                .chars()
                .chain(if invalid > 0 { Some('\u{fffd}') } else { None })
            {
                match escape {
                    0 => {
                        if ch == '\u{1b}' {
                            escape = 1;
                        } else if ch == '\n' || ch == '\t' || !ch.is_control() {
                            line.push(ch);
                        }
                    }
                    1 => {
                        escape = if ch == '[' {
                            2
                        } else if ch == ']' {
                            3
                        } else {
                            0
                        };
                    }
                    2 => {
                        if ('@'..='~').contains(&ch) {
                            escape = 0;
                        }
                    }
                    3 => {
                        if ch == '\u{7}' {
                            escape = 0;
                        } else if ch == '\u{1b}' {
                            escape = 4;
                        }
                    }
                    _ => {
                        escape = if ch == '\\' { 0 } else { 3 };
                    }
                }
                if line.ends_with('\n') || line.len() >= 8192 {
                    if sender
                        .send((kind.into(), std::mem::take(&mut line)))
                        .await
                        .is_err()
                    {
                        return Ok(());
                    }
                }
            }
            used += valid + invalid;
            if valid + invalid == 0 {
                break;
            }
        }
        pending.drain(..used);
        if count == 0 {
            if !line.is_empty() {
                let _ = sender.send((kind.into(), line)).await;
            }
            return Ok(());
        }
    }
}

#[cfg(test)]
mod pipe_tests {
    use super::*;
    use tokio::io::AsyncWriteExt;

    #[tokio::test]
    async fn split_utf8_escapes_and_partial_eof_are_decoded_without_losing_text() {
        // A one-byte pipe forces every multi-byte codepoint and escape across reads.
        let (mut writer, reader) = tokio::io::duplex(1);
        let input = "中文🙂\u{1b}[31m红色\u{1b}[0m\n\u{1b}]0;隐藏标题\u{7}尾部";
        let bytes = [input.as_bytes(), &[0xe4, 0xb8]].concat();
        let write = tokio::spawn(async move {
            writer.write_all(&bytes).await.unwrap();
        });
        let (sender, mut receiver) = mpsc::channel(64);
        read_pipe(reader, "stdout", sender).await.unwrap();
        write.await.unwrap();
        let mut output = String::new();
        while let Some((kind, text)) = receiver.recv().await {
            assert_eq!(kind, "stdout");
            output.push_str(&text);
        }
        assert_eq!(output, "中文🙂红色\n尾部�");
        assert_eq!(
            reported_progress("TAILTASK_PROGRESS {\"percent\":45.5}\n"),
            Some(45.5)
        );
        assert!(reported_progress("TAILTASK_PROGRESS {\"percent\":101}").is_none());
        assert!(reported_progress("ordinary output").is_none());
    }
}
