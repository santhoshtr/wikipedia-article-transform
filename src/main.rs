use anyhow::Context;
use clap::{Parser, Subcommand, ValueEnum};
use std::io::{self, Write};
use wikipedia_article_transform::{get_text, strip_references, ArticleFormat};

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
        /// Include citation references inline and as a reference list at the end
        #[arg(long, default_value_t = true, action = clap::ArgAction::Set)]
        include_references: bool,
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
            include_references,
        } => {
            let mut items = get_text(&language, &title).await.with_context(|| {
                format!("Failed to fetch '{title}' from {language}.wikipedia.org")
            })?;

            if !include_references {
                items = strip_references(items);
            }

            let stdout = io::stdout();
            let mut handle = stdout.lock();
            let output = match format {
                OutputFormat::Plain => items.format_plain(),
                OutputFormat::Json => items.format_json()?,
                OutputFormat::Markdown => items.format_markdown(),
            };
            writeln!(handle, "{output}")?;
        }
    }

    Ok(())
}
