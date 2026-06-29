use crate::config::{self, compute_fingerprint, mask_key, ApiKeyEntry};
use crate::errors::AppError;
use clap::{Args, Subcommand};
use serde_json::json;
use std::io::{self, Read};

#[derive(Debug, Args)]
pub struct ConfigArgs {
    #[command(subcommand)]
    pub action: ConfigAction,
}

#[derive(Debug, Subcommand)]
pub enum ConfigAction {
    /// Add an API key for a provider (reads from stdin to avoid shell history).
    AddKey {
        #[arg(long)]
        provider: String,
        #[arg(long, default_value_t = true)]
        from_stdin: bool,
        /// GAP-SG-34: no-op; JSON is always emitted on stdout.
        #[arg(long, hide = true)]
        json: bool,
    },
    /// List stored API keys (masked) with fingerprints.
    ListKeys {
        /// GAP-SG-34: no-op; JSON is always emitted on stdout.
        #[arg(long, hide = true)]
        json: bool,
    },
    /// Remove an API key by its fingerprint.
    RemoveKey {
        fingerprint: String,
        /// GAP-SG-34: no-op; JSON is always emitted on stdout.
        #[arg(long, hide = true)]
        json: bool,
    },
    /// Diagnose which layer won for each provider (env/config/cli).
    Doctor {
        /// GAP-SG-34: no-op; JSON is always emitted on stdout.
        #[arg(long, hide = true)]
        json: bool,
    },
    /// Print the resolved XDG config file path.
    Path {
        /// GAP-SG-34: no-op; JSON is always emitted on stdout.
        #[arg(long, hide = true)]
        json: bool,
    },
}

pub fn run(args: ConfigArgs) -> Result<(), AppError> {
    match args.action {
        ConfigAction::AddKey {
            provider,
            from_stdin,
            json: _,
        } => {
            let key = if from_stdin {
                let mut buf = String::new();
                io::stdin().read_to_string(&mut buf).map_err(AppError::Io)?;
                buf.trim().to_string()
            } else {
                return Err(AppError::Validation(
                    "--from-stdin is required to avoid shell history exposure".into(),
                ));
            };
            if key.is_empty() {
                return Err(AppError::Validation("API key cannot be empty".into()));
            }
            let fingerprint = compute_fingerprint(&key);
            let entry = ApiKeyEntry {
                provider: provider.clone(),
                value: key,
                added_at: chrono::Utc::now().to_rfc3339(),
                fingerprint: fingerprint.clone(),
            };
            let mut cfg = config::load_config()?;
            cfg.keys.retain(|k| k.provider != provider);
            cfg.keys.push(entry);
            config::save_config(&cfg)?;
            let output = json!({
                "action": "key_added",
                "provider": provider,
                "fingerprint": fingerprint,
            });
            println!("{}", serde_json::to_string(&output).unwrap());
            Ok(())
        }
        ConfigAction::ListKeys { json: _ } => {
            let cfg = config::load_config()?;
            let keys: Vec<_> = cfg
                .keys
                .iter()
                .map(|k| {
                    json!({
                        "provider": k.provider,
                        "fingerprint": k.fingerprint,
                        "masked_value": mask_key(&k.value),
                        "added_at": k.added_at,
                    })
                })
                .collect();
            let output = json!({ "keys": keys });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            Ok(())
        }
        ConfigAction::RemoveKey {
            fingerprint,
            json: _,
        } => {
            let mut cfg = config::load_config()?;
            let before = cfg.keys.len();
            cfg.keys.retain(|k| k.fingerprint != fingerprint);
            if cfg.keys.len() == before {
                return Err(AppError::NotFound(format!(
                    "no key with fingerprint {fingerprint}"
                )));
            }
            config::save_config(&cfg)?;
            let output = json!({
                "action": "key_removed",
                "fingerprint": fingerprint,
            });
            println!("{}", serde_json::to_string(&output).unwrap());
            Ok(())
        }
        ConfigAction::Doctor { json: _ } => {
            let config_path = config::config_file_path()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|_| "unavailable".to_string());
            let config_exists = std::path::Path::new(&config_path).exists();
            let providers = ["openrouter"];
            let mut results = vec![];
            for provider in &providers {
                let resolved = config::resolve_api_key(provider, None);
                results.push(json!({
                    "provider": provider,
                    "resolved": resolved.is_some(),
                    "source": resolved.as_ref().map(|r| r.source),
                    "masked_value": resolved.as_ref().map(|r| {
                        use secrecy::ExposeSecret;
                        mask_key(r.value.expose_secret())
                    }),
                }));
            }
            let output = json!({
                "config_path": config_path,
                "config_exists": config_exists,
                "providers": results,
            });
            println!("{}", serde_json::to_string_pretty(&output).unwrap());
            Ok(())
        }
        ConfigAction::Path { json: _ } => {
            let path = config::config_file_path()?;
            let output = json!({
                "config_path": path.display().to_string(),
                "exists": path.exists(),
            });
            println!("{}", serde_json::to_string(&output).unwrap());
            Ok(())
        }
    }
}
