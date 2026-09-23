use std::sync::Mutex;

const SERVICE: &str = "com.xiangwriter.remote";
static VAULT_LOCK: Mutex<()> = Mutex::new(());

// Never format a keyring error: BadEncoding can contain the stored secret.
fn failure(_: keyring::Error) -> String {
    "CREDENTIAL_STORE_UNAVAILABLE: 系统凭证库不可用；未降级为明文存储".into()
}

pub fn save(reference: &str, secret: &str) -> Result<(), String> {
    let _guard = VAULT_LOCK.lock().map_err(|_| "CREDENTIAL_STORE_LOCK")?;
    keyring::Entry::new(SERVICE, reference)
        .map_err(failure)?
        .set_password(secret)
        .map_err(failure)
}
pub fn read(reference: &str) -> Result<String, String> {
    let _guard = VAULT_LOCK.lock().map_err(|_| "CREDENTIAL_STORE_LOCK")?;
    keyring::Entry::new(SERVICE, reference)
        .map_err(failure)?
        .get_password()
        .map_err(failure)
}
pub fn remove(reference: &str) -> Result<(), String> {
    let _guard = VAULT_LOCK.lock().map_err(|_| "CREDENTIAL_STORE_LOCK")?;
    keyring::Entry::new(SERVICE, reference)
        .map_err(failure)?
        .delete_credential()
        .map_err(failure)
}
