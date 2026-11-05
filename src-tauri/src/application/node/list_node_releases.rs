use chrono::NaiveDate;
use std::sync::Arc;

use crate::domain::{
    errors::ApplicationError,
    node::{select_releases, NodeReleaseInfo},
    ports::node::{NodeManagerPort, NodeReleasesPort},
};

/// A release worth offering, with how it relates to what is installed.
#[derive(Debug, Clone)]
pub struct NodeReleaseView {
    pub release: NodeReleaseInfo,
    pub installed: bool,
    pub active: bool,
    /// An older version of the same major is installed, this one is not.
    pub update_available: bool,
    /// The line gets no more releases.
    pub end_of_life: bool,
}

pub struct ListNodeReleasesUseCase {
    manager: Arc<dyn NodeManagerPort>,
    releases: Arc<dyn NodeReleasesPort>,
}

impl ListNodeReleasesUseCase {
    pub fn new(manager: Arc<dyn NodeManagerPort>, releases: Arc<dyn NodeReleasesPort>) -> Self {
        Self { manager, releases }
    }

    pub async fn execute(&self, refresh: bool) -> Result<Vec<NodeReleaseView>, ApplicationError> {
        self.execute_on(refresh, chrono::Local::now().date_naive()).await
    }

    /// `execute` with an explicit "today" (the maintained-line cutoff depends on it).
    pub async fn execute_on(&self, refresh: bool, today: NaiveDate) -> Result<Vec<NodeReleaseView>, ApplicationError> {
        let all = self.releases.releases(refresh).await?;
        let env = self.manager.environment().await?;
        Ok(select_releases(&all, today).into_iter().map(|offer| {
            let release = offer.release;
            let installed = env.installed.iter().any(|i| i.version == release.version);
            let active = env.active.as_ref() == Some(&release.version);
            let update_available = !installed && env.installed.iter()
                .any(|i| i.version.major() == release.version.major() && i.version < release.version);
            NodeReleaseView { release, installed, active, update_available, end_of_life: offer.end_of_life }
        }).collect())
    }
}
