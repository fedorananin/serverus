use zeroize::Zeroizing;

use crate::error::AppResult;
use crate::vault::model::{Connection, FtpTlsMode};

pub struct FtpConfig {
    pub host: String,
    pub port: u16,
    pub username: String,
    pub password: Zeroizing<String>,
    pub tls: FtpTlsMode,
    pub passive: bool,
}

impl FtpConfig {
    pub fn from_connection(conn: &Connection) -> AppResult<Self> {
        let ftp = conn.ftp.clone().unwrap_or_default();
        Ok(Self {
            host: conn.host.clone(),
            port: conn.port,
            username: conn.auth.username.clone(),
            password: Zeroizing::new(conn.auth.password.clone().unwrap_or_default()),
            tls: ftp.tls,
            passive: ftp.passive,
        })
    }
}
