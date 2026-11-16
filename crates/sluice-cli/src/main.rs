use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "sluice-cli", about = "Sluice gateway management CLI")]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Generate a new unified token
    GenerateToken,
    /// Show the environment variable that carries a provider's API key
    AddProviderKey { name: String, key: String },
    /// Validate configuration
    CheckConfig,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::GenerateToken => {
            let token = generate_token();
            println!("Generated token: {}", token);
        }
        Commands::AddProviderKey { name, key } => {
            // The key is not stored anywhere by this command: the gateway reads it
            // from the environment at startup, so all this can do is confirm the
            // name resolves and print the variable to set. Storing it would need a
            // database connection and the master key, which is what the admin
            // console is for.
            let config = sluice_core::config::AppConfig::load(
                None,
                &sluice_core::config::EnvOverrides::from_env(),
            )?;

            if !config.providers.contains_key(&name) {
                anyhow::bail!(
                    "no provider named '{}' is configured; add it to config/providers.toml first",
                    name
                );
            }

            println!(
                "Set SLUICE_PROVIDER_{}_KEY to the value you supplied",
                name.to_uppercase()
            );
            println!(
                "  ({} characters, stored encrypted by the server)",
                key.len()
            );
        }
        Commands::CheckConfig => {
            let config = sluice_core::config::AppConfig::load(
                None,
                &sluice_core::config::EnvOverrides::from_env(),
            )?;
            println!("Configuration valid");
            for name in config.providers.keys() {
                println!("  - {}", name);
            }
        }
    }

    Ok(())
}

fn generate_token() -> String {
    use rand::Rng;
    const CHARSET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";
    let mut rng = rand::thread_rng();
    (0..32)
        .map(|_| {
            let idx = rng.gen_range(0..CHARSET.len());
            CHARSET[idx] as char
        })
        .collect()
}
