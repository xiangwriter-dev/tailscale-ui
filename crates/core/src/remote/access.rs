use super::{hex_digest, AgentStore};
use serde::{Deserialize, Serialize};
use sqlx::Row;
use subtle::ConstantTimeEq;

pub fn random_secret() -> Result<String, String> {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).map_err(|e| format!("RANDOM_UNAVAILABLE: {e}"))?;
    Ok(bytes.iter().map(|b| format!("{b:02x}")).collect())
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairingSession {
    pub id: String,
    pub secret: String,
    pub expires_at: i64,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairRequest {
    pub session_id: String,
    pub secret: String,
    pub controller_name: String,
}

#[derive(Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PairResponse {
    pub controller_id: String,
    pub token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AuthorizedController {
    pub id: String,
    pub name: String,
    pub created_at: String,
    pub revoked_at: Option<String>,
}

impl AgentStore {
    pub async fn create_pairing(&self) -> Result<PairingSession, String> {
        let session = PairingSession {
            id: uuid::Uuid::new_v4().to_string(),
            secret: random_secret()?,
            expires_at: chrono::Utc::now().timestamp() + 300,
        };
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("DELETE FROM agent_pairings")
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO agent_pairings(id,secret_hash,expires_at) VALUES(?,?,?)")
            .bind(&session.id)
            .bind(hex_digest(session.secret.as_bytes()))
            .bind(session.expires_at)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(session)
    }
    pub async fn pair(&self, request: &PairRequest) -> Result<PairResponse, String> {
        if request.controller_name.trim().is_empty()
            || request.controller_name.chars().count() > 120
            || request.controller_name.chars().any(char::is_control)
            || request.secret.len() > 128
            || request.session_id.len() > 64
        {
            return Err("INVALID_PAIR_REQUEST".into());
        }
        let mut tx = self
            .pool()
            .begin_with("BEGIN IMMEDIATE")
            .await
            .map_err(|e| e.to_string())?;
        let row = sqlx::query("SELECT * FROM agent_pairings WHERE id=?")
            .bind(&request.session_id)
            .fetch_optional(&mut *tx)
            .await
            .map_err(|e| e.to_string())?
            .ok_or("PAIRING_INVALID")?;
        if row.get::<i64, _>("consumed") != 0
            || row.get::<i64, _>("attempts") >= 5
            || row.get::<i64, _>("expires_at") <= chrono::Utc::now().timestamp()
        {
            return Err("PAIRING_EXPIRED_OR_USED".into());
        }
        let stored: String = row.get("secret_hash");
        let candidate = hex_digest(request.secret.as_bytes());
        if !bool::from(stored.as_bytes().ct_eq(candidate.as_bytes())) {
            sqlx::query("UPDATE agent_pairings SET attempts=attempts+1 WHERE id=?")
                .bind(&request.session_id)
                .execute(&mut *tx)
                .await
                .map_err(|e| e.to_string())?;
            tx.commit().await.map_err(|e| e.to_string())?;
            return Err("PAIRING_INVALID".into());
        }
        let response = PairResponse {
            controller_id: uuid::Uuid::new_v4().to_string(),
            token: random_secret()?,
        };
        sqlx::query("UPDATE agent_pairings SET consumed=1 WHERE id=?")
            .bind(&request.session_id)
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        sqlx::query("INSERT INTO agent_controllers(id,name,token_hash,created_at) VALUES(?,?,?,?)")
            .bind(&response.controller_id)
            .bind(&request.controller_name)
            .bind(hex_digest(response.token.as_bytes()))
            .bind(crate::now())
            .execute(&mut *tx)
            .await
            .map_err(|e| e.to_string())?;
        tx.commit().await.map_err(|e| e.to_string())?;
        Ok(response)
    }
    pub async fn authenticate(&self, token: &str) -> Result<String, String> {
        if token.len() != 64 {
            return Err("UNAUTHORIZED".into());
        }
        sqlx::query_scalar(
            "SELECT id FROM agent_controllers WHERE token_hash=? AND revoked_at IS NULL",
        )
        .bind(hex_digest(token.as_bytes()))
        .fetch_optional(self.pool())
        .await
        .map_err(|e| e.to_string())?
        .ok_or_else(|| "UNAUTHORIZED".into())
    }
    pub async fn revoke(&self, id: &str) -> Result<(), String> {
        let result = sqlx::query(
            "UPDATE agent_controllers SET revoked_at=COALESCE(revoked_at,?) WHERE id=?",
        )
        .bind(crate::now())
        .bind(id)
        .execute(self.pool())
        .await
        .map_err(|e| e.to_string())?;
        if result.rows_affected() == 0 {
            return Err("CONTROLLER_NOT_FOUND".into());
        }
        Ok(())
    }
    pub async fn controllers(&self) -> Result<Vec<AuthorizedController>, String> {
        Ok(sqlx::query(
            "SELECT id,name,created_at,revoked_at FROM agent_controllers ORDER BY created_at DESC",
        )
        .fetch_all(self.pool())
        .await
        .map_err(|e| e.to_string())?
        .into_iter()
        .map(|r| AuthorizedController {
            id: r.get("id"),
            name: r.get("name"),
            created_at: r.get("created_at"),
            revoked_at: r.get("revoked_at"),
        })
        .collect())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn pairing_is_single_use_expiring_rate_limited_and_revocable() {
        let dir = tempfile::tempdir().unwrap();
        let store = AgentStore::open(&dir.path().join("agent.db"))
            .await
            .unwrap();
        let session = store.create_pairing().await.unwrap();
        let request = PairRequest {
            session_id: session.id.clone(),
            secret: session.secret,
            controller_name: "控制端 A".into(),
        };
        let response = store.pair(&request).await.unwrap();
        assert_eq!(
            store.authenticate(&response.token).await.unwrap(),
            response.controller_id
        );
        assert!(store.pair(&request).await.is_err());
        assert!(store.authenticate(&random_secret().unwrap()).await.is_err());
        let persisted: String =
            sqlx::query_scalar("SELECT token_hash FROM agent_controllers WHERE id=?")
                .bind(&response.controller_id)
                .fetch_one(store.pool())
                .await
                .unwrap();
        assert_ne!(persisted, response.token);
        let s = store.create_pairing().await.unwrap();
        let request = PairRequest {
            session_id: s.id.clone(),
            secret: s.secret,
            controller_name: "控制端 B".into(),
        };
        let mut wrong = request.clone();
        wrong.secret = random_secret().unwrap();
        for _ in 0..5 {
            assert!(store.pair(&wrong).await.is_err());
        }
        assert!(store.pair(&request).await.is_err());
        let s = store.create_pairing().await.unwrap();
        sqlx::query("UPDATE agent_pairings SET expires_at=0 WHERE id=?")
            .bind(&s.id)
            .execute(store.pool())
            .await
            .unwrap();
        assert!(store
            .pair(&PairRequest {
                session_id: s.id,
                secret: s.secret,
                controller_name: "过期".into()
            })
            .await
            .is_err());
        store.revoke(&response.controller_id).await.unwrap();
        assert!(store.authenticate(&response.token).await.is_err());
    }
}
