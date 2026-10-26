//! The screen layout through a helper process (spec 0015): Session 0 cannot call the display
//! config API (ERROR_ACCESS_DENIED), so the service asks a copy of itself running as SYSTEM in
//! the console session. One request, one reply, both as JSON.

use mixlink_core::config::Topology;
use mixlink_proto::control::DisplayInfo;
use serde::{Deserialize, Serialize};

use super::ports::DisplayConfig;
use super::topology::{RestorePlan, SavedTarget, Snapshot};
use crate::error::SvcError;

/// One `DisplayConfig` call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "camelCase")]
pub enum AgentRequest {
    Snapshot,
    PresentMonitors,
    FindMixlink {
        product: u16,
    },
    HasPhysicalActive,
    SwitchTo {
        monitor_path: String,
        mode: DisplayInfo,
        topology: Topology,
        others: Vec<SavedTarget>,
    },
    Restore {
        plan: RestorePlan,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "reply", rename_all = "camelCase")]
pub enum AgentReply {
    Snapshot { snapshot: Snapshot },
    Paths { paths: Vec<String> },
    Found { path: Option<String> },
    Flag { value: bool },
    Done,
    Failed { message: String },
}

/// Helper side: runs `request` on `config` (the CCD API in the console session).
pub fn handle(config: &mut impl DisplayConfig, request: AgentRequest) -> AgentReply {
    let result = match request {
        AgentRequest::Snapshot => config
            .snapshot()
            .map(|snapshot| AgentReply::Snapshot { snapshot }),
        AgentRequest::PresentMonitors => config
            .present_monitors()
            .map(|paths| AgentReply::Paths { paths }),
        AgentRequest::FindMixlink { product } => config
            .find_mixlink(product)
            .map(|path| AgentReply::Found { path }),
        AgentRequest::HasPhysicalActive => config
            .has_physical_active()
            .map(|value| AgentReply::Flag { value }),
        AgentRequest::SwitchTo {
            monitor_path,
            mode,
            topology,
            others,
        } => config
            .switch_to(&monitor_path, &mode, topology, &others)
            .map(|()| AgentReply::Done),
        AgentRequest::Restore { plan } => config.restore(&plan).map(|()| AgentReply::Done),
    };
    result.unwrap_or_else(|error| AgentReply::Failed {
        message: error.to_string(),
    })
}

/// Delivers one request to the helper and brings its reply back.
pub trait AgentRunner {
    fn call(&mut self, request: &AgentRequest) -> Result<AgentReply, SvcError>;
}

/// Service side: `DisplayConfig` answered by the helper.
#[derive(Debug)]
pub struct AgentConfig<R: AgentRunner> {
    runner: R,
}

impl<R: AgentRunner> AgentConfig<R> {
    pub fn new(runner: R) -> AgentConfig<R> {
        AgentConfig { runner }
    }

    fn call(&mut self, request: &AgentRequest) -> Result<AgentReply, SvcError> {
        match self.runner.call(request)? {
            AgentReply::Failed { message } => Err(SvcError::DisplayAgent(message)),
            reply => Ok(reply),
        }
    }
}

fn unexpected(reply: &AgentReply) -> SvcError {
    SvcError::DisplayAgent(format!("unexpected reply {reply:?}"))
}

impl<R: AgentRunner> DisplayConfig for AgentConfig<R> {
    fn snapshot(&mut self) -> Result<Snapshot, SvcError> {
        match self.call(&AgentRequest::Snapshot)? {
            AgentReply::Snapshot { snapshot } => Ok(snapshot),
            other => Err(unexpected(&other)),
        }
    }

    fn present_monitors(&mut self) -> Result<Vec<String>, SvcError> {
        match self.call(&AgentRequest::PresentMonitors)? {
            AgentReply::Paths { paths } => Ok(paths),
            other => Err(unexpected(&other)),
        }
    }

    fn find_mixlink(&mut self, product: u16) -> Result<Option<String>, SvcError> {
        match self.call(&AgentRequest::FindMixlink { product })? {
            AgentReply::Found { path } => Ok(path),
            other => Err(unexpected(&other)),
        }
    }

    fn has_physical_active(&mut self) -> Result<bool, SvcError> {
        match self.call(&AgentRequest::HasPhysicalActive)? {
            AgentReply::Flag { value } => Ok(value),
            other => Err(unexpected(&other)),
        }
    }

    fn switch_to(
        &mut self,
        monitor_path: &str,
        mode: &DisplayInfo,
        topology: Topology,
        others: &[SavedTarget],
    ) -> Result<(), SvcError> {
        let request = AgentRequest::SwitchTo {
            monitor_path: monitor_path.to_owned(),
            mode: *mode,
            topology,
            others: others.to_vec(),
        };
        match self.call(&request)? {
            AgentReply::Done => Ok(()),
            other => Err(unexpected(&other)),
        }
    }

    fn restore(&mut self, plan: &RestorePlan) -> Result<(), SvcError> {
        match self.call(&AgentRequest::Restore { plan: plan.clone() })? {
            AgentReply::Done => Ok(()),
            other => Err(unexpected(&other)),
        }
    }
}

#[cfg(test)]
#[path = "agent_tests.rs"]
mod tests;
