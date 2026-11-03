//! The behavior of the test-only provider (`crate::providers::test_provider`): only the launch
//! plan, which every provider must have, so every other sub-interface is the trait's default.

use anyhow::Result;

use super::{AgentProvider, LaunchPlan, LaunchSpec};
use crate::providers::{AgentId, ProviderDescriptor};

pub struct TestProvider;

impl AgentProvider for TestProvider {
    fn descriptor(&self) -> &'static ProviderDescriptor {
        AgentId::TEST.descriptor()
    }

    fn plan(&self, _spec: &LaunchSpec<'_>) -> Result<LaunchPlan> {
        Ok(LaunchPlan {
            args: Vec::new(),
            private_server: None,
            env: Vec::new(),
            resolves_approvals_itself: false,
            notice: None,
            carried_trust: None,
            after_exit: None,
        })
    }
}
