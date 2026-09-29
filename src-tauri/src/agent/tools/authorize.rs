//! Checking an action against the access policy, and asking the user in
//! Serverus when the policy says so.

use std::time::{Duration, Instant};

use serverus_domain::agent::access::{
    decide, decide_new_connection, AccessLevel, Decision, OperationClass,
};

use super::context::Policy;
use super::journal::DECLINED;
use super::{Ctx, ToolError};
use crate::agent::catalog::ServerInfo;
use crate::agent::types::{AgentConfirmDecision, AgentUiRequest, AgentUiResponse};
use crate::agent::unlock;

const CONFIRM_TIMEOUT: Duration = Duration::from_secs(600);

/// Grant key of "allow adding connections for a while".
const CREATE_GRANT: &str = "vault:add-connection";

impl Ctx<'_> {
    /// Refuse early what the level denies outright, before opening a tab.
    pub fn precheck(&self, server: &ServerInfo, class: OperationClass) -> Result<(), ToolError> {
        if decide(server.level, class, false) == Decision::Deny {
            return Err(ToolError(format!(
                "Your access to `{}` is read-only: you can list and read, but not change anything or run commands. The user can raise it in the connection settings.",
                server.path()
            )));
        }
        Ok(())
    }

    /// Check an action against the server's level, asking the user in
    /// Serverus when the level says so.
    pub async fn authorize(
        &self,
        policy: &Policy,
        server: &ServerInfo,
        class: OperationClass,
        action: &str,
        detail: &str,
    ) -> Result<(), ToolError> {
        self.precheck(server, class)?;
        let hub = self.hub();
        let granted = hub
            .grants
            .is_granted(&server.id, &policy.epoch, Instant::now());
        if decide(server.level, class, granted) == Decision::Allow {
            return Ok(());
        }
        self.ask_user(policy, &server.id, &server.path(), action, detail)
            .await
    }

    /// Check an "add a connection" request against the vault's create mode
    /// and what the new connection would give the agent: `level` on it,
    /// routed through a jump host at `jump_level`.
    pub async fn authorize_create(
        &self,
        policy: &Policy,
        detail: &str,
        level: AccessLevel,
        jump_level: Option<AccessLevel>,
    ) -> Result<(), ToolError> {
        let granted = self
            .hub()
            .grants
            .is_granted(CREATE_GRANT, &policy.epoch, Instant::now());
        let mode = policy.create_mode.into();
        match decide_new_connection(mode, policy.full_access, granted, level, jump_level) {
            Decision::Allow => Ok(()),
            Decision::Deny => Err(
                "The user has not allowed you to add connections. They can allow it in Serverus → Settings → AI Agent → \"Agent may add connections\"."
                    .into(),
            ),
            Decision::Confirm => {
                self.ask_user(policy, CREATE_GRANT, "Serverus", "Add a connection", detail)
                    .await
            }
        }
    }

    /// Show the confirmation dialog. "For a while" grants `grant_key`.
    async fn ask_user(
        &self,
        policy: &Policy,
        grant_key: &str,
        subject: &str,
        action: &str,
        detail: &str,
    ) -> Result<(), ToolError> {
        unlock::request_attention(self.app, false);
        let request = AgentUiRequest::Confirm {
            connection_id: grant_key.to_string(),
            server: subject.to_string(),
            action: action.to_string(),
            detail: detail.to_string(),
        };
        let timeout = format!(
            "The user did not answer the confirmation in Serverus within {} minutes, so the action was not done. Ask them before trying again.",
            CONFIRM_TIMEOUT.as_secs() / 60
        );
        match self.ui(request, CONFIRM_TIMEOUT, &timeout).await? {
            AgentUiResponse::Confirm {
                decision: AgentConfirmDecision::Once,
            } => Ok(()),
            AgentUiResponse::Confirm {
                decision: AgentConfirmDecision::ForAWhile,
            } => {
                self.hub()
                    .grants
                    .grant(grant_key, &policy.epoch, Instant::now());
                Ok(())
            }
            _ => Err(ToolError(format!(
                "{DECLINED} this action on `{subject}`. Do not retry it unless they ask you to."
            ))),
        }
    }
}
