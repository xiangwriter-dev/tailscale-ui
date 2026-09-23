use clap::{Parser, Subcommand};
use std::path::PathBuf;
use tailtask_core::{RepairAction, RepairRequest, Store};

#[derive(Parser)]
#[command(
    version,
    about = "xiangwriter远程器：本机维护与显式开启的远程任务执行端"
)]
struct Cli {
    #[arg(long)]
    data_dir: PathBuf,
    #[command(subcommand)]
    action: Action,
}

#[derive(Subcommand)]
enum Action {
    Serve,
    CheckDatabase,
    RebuildIndexes,
    History,
}

#[tokio::main]
async fn main() {
    tailtask_remote::process::worker_entry();
    if let Err(error) = run().await {
        eprintln!("{error}");
        std::process::exit(1);
    }
}

async fn run() -> Result<(), String> {
    let cli = Cli::parse();
    if matches!(cli.action, Action::Serve) {
        return tailtask_remote::service::serve(cli.data_dir).await;
    }
    let store = Store::open(&cli.data_dir.join("agent.db")).await?;
    match cli.action {
        Action::History => println!(
            "{}",
            serde_json::to_string_pretty(&store.tasks(50, 0).await?).map_err(|e| e.to_string())?
        ),
        action => {
            let action = match action {
                Action::CheckDatabase => RepairAction::CheckDatabase,
                _ => RepairAction::RebuildIndexes,
            };
            let (task, _) = store
                .submit(&RepairRequest {
                    request_id: uuid::Uuid::new_v4().to_string(),
                    action,
                })
                .await?;
            store.run(&task.id).await?;
            let detail = store.detail(&task.id).await?;
            println!(
                "{}",
                serde_json::to_string_pretty(&detail).map_err(|e| e.to_string())?
            );
            if detail.task.state != "succeeded" {
                return Err("REPAIR_FAILED: 请查看任务结果".into());
            }
        }
    }
    Ok(())
}
