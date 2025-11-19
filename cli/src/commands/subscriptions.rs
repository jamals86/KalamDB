use crate::args::Cli;
use crate::connect::create_session;
use kalam_cli::{CLIConfiguration, FileCredentialStore, Result};

pub async fn handle_subscriptions(
    cli: &Cli,
    credential_store: &FileCredentialStore,
) -> Result<bool> {
    if cli.list_subscriptions || cli.subscribe.is_some() || cli.unsubscribe.is_some() {
        // Load configuration
        let config = CLIConfiguration::load(&cli.config)?;

        let mut session = create_session(cli, credential_store, &config).await?;

        if cli.list_subscriptions {
            session.list_subscriptions().await?;
        } else if let Some(query) = &cli.subscribe {
            session.subscribe(query).await?;
        } else if let Some(subscription_id) = &cli.unsubscribe {
            session.unsubscribe(subscription_id).await?;
        }

        return Ok(true);
    }

    Ok(false)
}
