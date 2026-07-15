use crate::vault::model::TunnelConfig;

use super::super::{ftp, s3};

pub(crate) enum ConnectionPlan {
    Ssh {
        chain: Vec<super::super::ssh::Hop>,
        autostart_tunnels: Vec<TunnelConfig>,
    },
    Ftp {
        config: ftp::FtpConfig,
        max_parallel: usize,
    },
    S3 {
        config: s3::S3Config,
    },
}

impl ConnectionPlan {
    pub(crate) fn autostart_tunnels(&self) -> &[TunnelConfig] {
        match self {
            Self::Ssh {
                autostart_tunnels, ..
            } => autostart_tunnels,
            Self::Ftp { .. } | Self::S3 { .. } => &[],
        }
    }
}
