//! Shell completion script generation.

use clap::CommandFactory;
use clap_complete::{generate, Shell};

#[derive(clap::Args, Debug)]
pub struct CompletionsArgs {
    /// Shell to generate completions for
    #[arg(value_enum)]
    pub shell: Shell,
}

pub fn run(args: CompletionsArgs) -> Result<(), crate::errors::AppError> {
    let mut cmd = crate::cli::Cli::command();
    let bin_name = cmd.get_name().to_string();
    generate(args.shell, &mut cmd, bin_name, &mut std::io::stdout());
    Ok(())
}
