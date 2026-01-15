use crate::args::Cli;
use crate::connect::create_session;
use kalam_cli::{CLIConfiguration, FileCredentialStore, Result};
use std::time::Duration;

pub async fn handle_subscriptions(
    cli: &Cli,
    credential_store: &mut FileCredentialStore,
) -> Result<bool> {
    if cli.list_subscriptions || cli.subscribe.is_some() || cli.unsubscribe.is_some() {
        // Load configuration
        let config = CLIConfiguration::load(&cli.config)?;

        let config_path = kalam_cli::config::expand_config_path(&cli.config);
        let mut session = create_session(cli, credential_store, &config, config_path).await?;

        if cli.list_subscriptions {
            session.list_subscriptions().await?;
        } else if let Some(query) = &cli.subscribe {
            // Convert timeout from seconds to Duration (0 = no timeout)
            let timeout = if cli.subscription_timeout > 0 {
                Some(Duration::from_secs(cli.subscription_timeout))
            } else {
                None
            };
            session.subscribe_with_timeout(query, timeout).await?;
        } else if let Some(subscription_id) = &cli.unsubscribe {
            session.unsubscribe(subscription_id).await?;
        }

        return Ok(true);
    }

    Ok(false)
}
