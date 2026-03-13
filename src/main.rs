use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::{self, Write};
use wikipedia_article_transform::{get_text, ArticleFormat};

#[derive(Parser, Debug)]
#[command(author, version, about = "Extract plain text from Wikipedia HTML")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand, Debug)]
enum Command {
    /// Fetch a Wikipedia article by language and title, print extracted text.
    Fetch {
        /// Wikipedia language code (e.g. "en", "ml")
        #[arg(short, long)]
        language: String,
        /// Wikipedia article title
        #[arg(short, long)]
        title: String,
        /// Output format
        #[arg(short, long, default_value = "plain")]
        format: OutputFormat,
    },
}

#[derive(Debug, Clone, ValueEnum)]
enum OutputFormat {
    /// Plain text with section headings
    Plain,
    /// Semantic JSON with section tree
    Json,
    /// Markdown with inline bold/italic/link formatting
    Markdown,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Command::Fetch {
            language,
            title,
            format,
        } => {
            let segments = get_text(&language, &title).await.with_context(|| {
                format!("Failed to fetch '{title}' from {language}.wikipedia.org")
            })?;

            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let output = match format {
                OutputFormat::Plain => segments.format_plain(),
                OutputFormat::Json => segments.format_json()?,
                OutputFormat::Markdown => segments.format_markdown(),
            };
            writeln!(handle, "{output}")?;
        }
    }

    Ok(())
}
