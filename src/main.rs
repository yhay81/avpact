use std::path::PathBuf;
use std::process::ExitCode;

use avpact::error::{AvpactError, ErrorDocument};
use clap::{CommandFactory, Parser, Subcommand, ValueEnum};

#[derive(Debug, Parser)]
#[command(name = "avpact", version, about)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    /// Show the schema catalog or one full JSON Schema.
    Schema {
        /// Emit the compact schema catalog.
        #[arg(long, conflicts_with = "document")]
        brief: bool,

        /// Emit the full schema for one document type.
        #[arg(long, value_enum)]
        document: Option<SchemaDocument>,

        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },

    /// Inspect FFmpeg/FFprobe capabilities without modifying media.
    Capabilities {
        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        /// FFmpeg executable or path.
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: PathBuf,

        /// FFprobe executable or path.
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },

    /// Generate a shell completion script on stdout.
    Completions {
        /// Target shell.
        #[arg(value_enum)]
        shell: CompletionShell,
    },

    /// Inspect a local media file without modifying it.
    Inspect {
        /// Input media path.
        input: PathBuf,

        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        /// FFprobe executable or path.
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },

    /// Compile a read-only recipe into an inspectable execution plan.
    Plan {
        /// JSON recipe path.
        recipe: PathBuf,

        /// Destination for the immutable plan JSON document.
        #[arg(long)]
        out: PathBuf,

        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        /// FFmpeg executable or path.
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: PathBuf,

        /// FFprobe executable or path.
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },

    /// Execute a validated plan and publish only a verified output.
    Apply {
        /// Plan JSON path.
        plan: PathBuf,

        /// Destination for the resulting receipt JSON.
        #[arg(long, conflicts_with = "state_dir")]
        receipt_out: Option<PathBuf>,

        /// Receipt store directory; defaults to .avpact beside the plan.
        #[arg(long, conflicts_with = "receipt_out")]
        state_dir: Option<PathBuf>,

        /// Structured progress stream written to stderr.
        #[arg(long, value_enum, default_value_t = ProgressFormat::Ndjson)]
        progress: ProgressFormat,

        /// Structured final output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        /// FFmpeg executable or path.
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: PathBuf,

        /// FFprobe executable or path.
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,
    },

    /// Read durable execution receipts.
    Receipt {
        #[command(subcommand)]
        command: ReceiptCommand,
    },

    /// Verify a media output against an immutable plan.
    Verify {
        /// Media output path.
        output: PathBuf,

        /// Plan JSON path.
        #[arg(long)]
        against: PathBuf,

        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,

        /// FFprobe executable or path.
        #[arg(long, default_value = "ffprobe")]
        ffprobe: PathBuf,

        /// FFmpeg executable or path, used by measurement-based checks.
        #[arg(long, default_value = "ffmpeg")]
        ffmpeg: PathBuf,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum OutputFormat {
    Json,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum ProgressFormat {
    Ndjson,
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum CompletionShell {
    Bash,
    Elvish,
    Fish,
    PowerShell,
    Zsh,
}

#[derive(Debug, Subcommand)]
enum ReceiptCommand {
    /// Show one receipt from the local receipt store.
    Show {
        /// Receipt identifier returned by apply.
        receipt_id: String,

        /// Receipt store directory.
        #[arg(long, default_value = ".avpact")]
        state_dir: PathBuf,

        /// Structured output format.
        #[arg(long, value_enum, default_value_t = OutputFormat::Json)]
        format: OutputFormat,
    },
}

#[derive(Copy, Clone, Debug, ValueEnum)]
enum SchemaDocument {
    Recipe,
    Plan,
    Inspection,
    Progress,
    Verification,
    Receipt,
    Error,
    Capability,
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(exit_code) => exit_code,
        Err(error) => {
            write_error(&error);
            ExitCode::FAILURE
        }
    }
}

fn run(cli: Cli) -> Result<ExitCode, AvpactError> {
    match cli.command {
        Command::Schema {
            brief: _,
            document,
            format: OutputFormat::Json,
        } => {
            let value = match document {
                Some(document) => avpact::schema::document(document.into())?,
                None => avpact::schema::catalog(),
            };
            println!("{}", serde_json::to_string_pretty(&value)?);
        }
        Command::Capabilities {
            format: OutputFormat::Json,
            ffmpeg,
            ffprobe,
        } => {
            let report = avpact::capability::inspect_capabilities(&ffmpeg, &ffprobe)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Completions { shell } => {
            let mut command = Cli::command();
            clap_complete::generate(
                clap_complete::Shell::from(shell),
                &mut command,
                "avpact",
                &mut std::io::stdout(),
            );
        }
        Command::Inspect {
            input,
            format: OutputFormat::Json,
            ffprobe,
        } => {
            let report = avpact::inspect::inspect(&input, &ffprobe)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
        }
        Command::Plan {
            recipe,
            out,
            format: OutputFormat::Json,
            ffmpeg,
            ffprobe,
        } => {
            let plan = avpact::plan::plan_recipe(&recipe, &ffmpeg, &ffprobe)?;
            let json = serde_json::to_string_pretty(&plan)?;
            avpact::plan::write_new_plan(&out, &json)?;
            println!("{json}");
        }
        Command::Apply {
            plan,
            receipt_out,
            state_dir,
            progress: ProgressFormat::Ndjson,
            format: OutputFormat::Json,
            ffmpeg,
            ffprobe,
        } => {
            let cancellation = avpact::apply::CancellationToken::new();
            let signal_token = cancellation.clone();
            ctrlc::set_handler(move || signal_token.cancel()).map_err(|error| {
                AvpactError::CancellationSetup {
                    message: error.to_string(),
                }
            })?;
            let receipt = if let Some(receipt_out) = receipt_out {
                avpact::apply::apply_plan_with_cancellation(
                    &plan,
                    &receipt_out,
                    &ffmpeg,
                    &ffprobe,
                    &cancellation,
                    emit_progress,
                )?
            } else {
                let state_dir = state_dir.unwrap_or_else(|| {
                    plan.parent()
                        .unwrap_or_else(|| std::path::Path::new("."))
                        .join(".avpact")
                });
                avpact::apply::apply_plan_to_store_with_cancellation(
                    &plan,
                    &state_dir,
                    &ffmpeg,
                    &ffprobe,
                    &cancellation,
                    emit_progress,
                )?
            };
            println!("{}", serde_json::to_string_pretty(&receipt)?);
        }
        Command::Receipt {
            command:
                ReceiptCommand::Show {
                    receipt_id,
                    state_dir,
                    format: OutputFormat::Json,
                },
        } => {
            let receipt = avpact::apply::read_stored_receipt(&state_dir, &receipt_id)?;
            println!("{}", serde_json::to_string_pretty(&receipt)?);
        }
        Command::Verify {
            output,
            against,
            format: OutputFormat::Json,
            ffprobe,
            ffmpeg,
        } => {
            let plan = avpact::plan::read_plan(&against)?;
            let report = avpact::apply::verify_output(&output, &plan, &ffmpeg, &ffprobe)?;
            println!("{}", serde_json::to_string_pretty(&report)?);
            return Ok(if report.passed {
                ExitCode::SUCCESS
            } else {
                ExitCode::from(2)
            });
        }
    }
    Ok(ExitCode::SUCCESS)
}

impl From<CompletionShell> for clap_complete::Shell {
    fn from(shell: CompletionShell) -> Self {
        match shell {
            CompletionShell::Bash => Self::Bash,
            CompletionShell::Elvish => Self::Elvish,
            CompletionShell::Fish => Self::Fish,
            CompletionShell::PowerShell => Self::PowerShell,
            CompletionShell::Zsh => Self::Zsh,
        }
    }
}

fn emit_progress(event: &avpact::apply::ProgressEvent) {
    if let Ok(json) = serde_json::to_string(event) {
        eprintln!("{json}");
    }
}

impl From<SchemaDocument> for avpact::schema::Document {
    fn from(document: SchemaDocument) -> Self {
        match document {
            SchemaDocument::Recipe => Self::Recipe,
            SchemaDocument::Plan => Self::Plan,
            SchemaDocument::Inspection => Self::Inspection,
            SchemaDocument::Progress => Self::Progress,
            SchemaDocument::Verification => Self::Verification,
            SchemaDocument::Receipt => Self::Receipt,
            SchemaDocument::Error => Self::Error,
            SchemaDocument::Capability => Self::Capability,
        }
    }
}

fn write_error(error: &AvpactError) {
    let document = ErrorDocument::from(error);
    match serde_json::to_string(&document) {
        Ok(json) => eprintln!("{json}"),
        Err(_) => eprintln!(
            r#"{{"schema_version":"{}","error":{{"code":"serialization_failed","message":"failed to serialize error"}}}}"#,
            avpact::ERROR_SCHEMA_VERSION
        ),
    }
}
