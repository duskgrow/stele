use std::sync::Arc;

use clap::{Parser, Subcommand};
use serde_json::json;

use crate::ops::{Operation, OperationRegistry};
use crate::types::Result;

/// Command-line interface for the stele knowledge manager.
#[derive(Parser, Debug)]
#[command(name = "stele")]
#[command(about = "Stele CLI - Personal knowledge management")]
pub struct SteleCli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug)]
pub enum Commands {
    /// Start the MCP server
    Serve {
        /// Transport type
        #[arg(long, value_name = "TRANSPORT", default_value = "stdio")]
        transport: String,
        /// Port for HTTP transport
        #[arg(long, value_name = "PORT", default_value = "3000")]
        port: u16,
    },
    /// Page operations
    Page {
        #[command(subcommand)]
        command: PageCommands,
    },
    /// Full-text search
    Search {
        /// Search query
        query: String,
        /// Result limit
        #[arg(long, value_name = "N")]
        limit: Option<i64>,
        /// Type filter
        #[arg(long, value_name = "TYPE")]
        type_filter: Option<String>,
    },
    /// Graph query
    Graph {
        /// Page slug
        slug: String,
        /// Query depth
        #[arg(long, value_name = "N")]
        depth: Option<usize>,
    },
    /// Synchronize from FNS vault
    Sync {
        /// Root directory
        #[arg(long, value_name = "DIR")]
        dir: Option<String>,
    },
    /// Run maintenance tasks
    Maintain {
        /// Maintenance scope
        #[arg(long, value_name = "SCOPE", default_value = "full")]
        scope: String,
    },
    /// Get index statistics
    Stats,
    /// Rebuild search index
    Reindex,
}

#[derive(Subcommand, Debug)]
pub enum PageCommands {
    /// Get a page by slug
    Get { slug: String },
    /// Create or update a page
    Put {
        slug: String,
        /// Content from file
        #[arg(long, value_name = "PATH")]
        file: Option<String>,
        /// Content as text
        #[arg(long, value_name = "TEXT")]
        content: Option<String>,
    },
    /// Delete a page
    Delete { slug: String },
    /// List pages
    List {
        /// Directory filter
        dir: Option<String>,
    },
}

fn command_to_operation(cmd: Commands) -> Result<Operation> {
    let op = match cmd {
        Commands::Serve { .. } => {
            return Err(crate::types::Error::Config(
                "serve is handled separately, not dispatched through OperationRegistry".into(),
            ));
        }
        Commands::Page { command } => match command {
            PageCommands::Get { slug } => Operation::PageGet { slug },
            PageCommands::Put { slug, file, content } => {
                let content_str = match (file, content) {
                    (Some(path), None) => std::fs::read_to_string(&path)?,
                    (None, Some(text)) => text,
                    (Some(_), Some(_)) => {
                        return Err(crate::types::Error::Config(
                            "cannot specify both --file and --content".into(),
                        ));
                    }
                    (None, None) => {
                        return Err(crate::types::Error::Config(
                            "must specify either --file or --content".into(),
                        ));
                    }
                };
                Operation::PagePut {
                    slug,
                    content: content_str,
                    etag: None,
                }
            }
            PageCommands::Delete { slug } => Operation::PageDelete { slug },
            PageCommands::List { dir } => Operation::PageList { dir },
        },
        Commands::Search {
            query,
            limit,
            type_filter,
        } => Operation::Search {
            query,
            limit,
            type_filter,
        },
        Commands::Graph { slug, depth } => Operation::GraphQuery { slug, depth },
        Commands::Sync { dir } => Operation::Sync { dir },
        Commands::Maintain { scope } => Operation::Maintain {
            scope: Some(scope),
        },
        Commands::Stats => Operation::Stats,
        Commands::Reindex => Operation::Reindex,
    };
    Ok(op)
}

/// Parse CLI arguments and dispatch to the operation registry.
pub async fn run_cli(registry: Arc<OperationRegistry>) -> Result<()> {
    let cli = match SteleCli::try_parse() {
        Ok(c) => c,
        Err(e) => {
            let err_json = json!({"error": e.to_string()});
            eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap_or_default());
            std::process::exit(1);
        }
    };

    match cli.command {
        Commands::Serve { transport, port } => {
            if transport == "stdio" {
                crate::mcp::stdio::run_stdio(registry).await
            } else {
                let result = json!({
                    "status": "serving",
                    "transport": transport,
                    "port": port
                });
                println!("{}", serde_json::to_string_pretty(&result)?);
                Ok(())
            }
        }
        cmd => {
            let op = command_to_operation(cmd)?;
            match registry.execute(op).await {
                Ok(val) => {
                    println!("{}", serde_json::to_string_pretty(&val)?);
                    Ok(())
                }
                Err(e) => {
                    let err_json = json!({"error": e.to_string()});
                    eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap_or_default());
                    std::process::exit(1);
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn test_cli_parses_serve() {
        let cli = SteleCli::parse_from(["stele", "serve", "--transport", "http", "--port", "8080"]);
        match cli.command {
            Commands::Serve { transport, port } => {
                assert_eq!(transport, "http");
                assert_eq!(port, 8080);
            }
            other => panic!("expected Serve, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_serve_defaults() {
        let cli = SteleCli::parse_from(["stele", "serve"]);
        match cli.command {
            Commands::Serve { transport, port } => {
                assert_eq!(transport, "stdio");
                assert_eq!(port, 3000);
            }
            other => panic!("expected Serve, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_page_get() {
        let cli = SteleCli::parse_from(["stele", "page", "get", "hello-world"]);
        match cli.command {
            Commands::Page { command } => match command {
                PageCommands::Get { slug } => {
                    assert_eq!(slug, "hello-world");
                }
                other => panic!("expected Get, got {:?}", other),
            },
            other => panic!("expected Page, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_search() {
        let cli = SteleCli::parse_from(["stele", "search", "hello", "--limit", "10", "--type-filter", "note"]);
        match cli.command {
            Commands::Search {
                query,
                limit,
                type_filter,
            } => {
                assert_eq!(query, "hello");
                assert_eq!(limit, Some(10));
                assert_eq!(type_filter, Some("note".to_string()));
            }
            other => panic!("expected Search, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_stats() {
        let cli = SteleCli::parse_from(["stele", "stats"]);
        match cli.command {
            Commands::Stats => {}
            other => panic!("expected Stats, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_maintain() {
        let cli = SteleCli::parse_from(["stele", "maintain", "--scope", "lint"]);
        match cli.command {
            Commands::Maintain { scope } => {
                assert_eq!(scope, "lint");
            }
            other => panic!("expected Maintain, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_graph() {
        let cli = SteleCli::parse_from(["stele", "graph", "foo", "--depth", "2"]);
        match cli.command {
            Commands::Graph { slug, depth } => {
                assert_eq!(slug, "foo");
                assert_eq!(depth, Some(2));
            }
            other => panic!("expected Graph, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_page_put_content() {
        let cli = SteleCli::parse_from(["stele", "page", "put", "foo", "--content", "hello"]);
        match cli.command {
            Commands::Page { command } => match command {
                PageCommands::Put { slug, content, file } => {
                    assert_eq!(slug, "foo");
                    assert_eq!(content, Some("hello".to_string()));
                    assert_eq!(file, None);
                }
                other => panic!("expected Put, got {:?}", other),
            },
            other => panic!("expected Page, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_sync() {
        let cli = SteleCli::parse_from(["stele", "sync", "--dir", "/tmp/vault"]);
        match cli.command {
            Commands::Sync { dir } => {
                assert_eq!(dir, Some("/tmp/vault".to_string()));
            }
            other => panic!("expected Sync, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_reindex() {
        let cli = SteleCli::parse_from(["stele", "reindex"]);
        match cli.command {
            Commands::Reindex => {}
            other => panic!("expected Reindex, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_help() {
        // --help exits with code 0; catch the Clap error which is printed to stdout
        let result = SteleCli::try_parse_from(["stele", "--help"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Stele CLI"), "help should mention 'Stele CLI': {}", msg);
    }

    #[test]
    fn test_cli_command_to_operation_page_get() {
        let cmd = Commands::Page {
            command: PageCommands::Get {
                slug: "test".into(),
            },
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::PageGet { slug } => assert_eq!(slug, "test"),
            other => panic!("expected PageGet, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_stats() {
        let cmd = Commands::Stats;
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::Stats => {}
            other => panic!("expected Stats, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_reindex() {
        let cmd = Commands::Reindex;
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::Reindex => {}
            other => panic!("expected Reindex, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_page_put_content() {
        let cmd = Commands::Page {
            command: PageCommands::Put {
                slug: "foo".into(),
                file: None,
                content: Some("hello".into()),
            },
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::PagePut { slug, content, etag } => {
                assert_eq!(slug, "foo");
                assert_eq!(content, "hello");
                assert_eq!(etag, None);
            }
            other => panic!("expected PagePut, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_page_put_both_file_and_content_fails() {
        let cmd = Commands::Page {
            command: PageCommands::Put {
                slug: "foo".into(),
                file: Some("/tmp/test.md".into()),
                content: Some("hello".into()),
            },
        };
        let result = command_to_operation(cmd);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("cannot specify both"));
    }

    #[test]
    fn test_cli_command_to_operation_page_put_neither_fails() {
        let cmd = Commands::Page {
            command: PageCommands::Put {
                slug: "foo".into(),
                file: None,
                content: None,
            },
        };
        let result = command_to_operation(cmd);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("must specify either"));
    }

    #[test]
    fn test_cli_command_to_operation_page_delete() {
        let cmd = Commands::Page {
            command: PageCommands::Delete {
                slug: "delete-me".into(),
            },
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::PageDelete { slug } => assert_eq!(slug, "delete-me"),
            other => panic!("expected PageDelete, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_page_list() {
        let cmd = Commands::Page {
            command: PageCommands::List {
                dir: Some("notes".into()),
            },
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::PageList { dir } => assert_eq!(dir, Some("notes".into())),
            other => panic!("expected PageList, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_search() {
        let cmd = Commands::Search {
            query: "rust".into(),
            limit: Some(10),
            type_filter: Some("note".into()),
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::Search {
                query,
                limit,
                type_filter,
            } => {
                assert_eq!(query, "rust");
                assert_eq!(limit, Some(10));
                assert_eq!(type_filter, Some("note".into()));
            }
            other => panic!("expected Search, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_graph() {
        let cmd = Commands::Graph {
            slug: "foo".into(),
            depth: Some(2),
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::GraphQuery { slug, depth } => {
                assert_eq!(slug, "foo");
                assert_eq!(depth, Some(2));
            }
            other => panic!("expected GraphQuery, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_sync() {
        let cmd = Commands::Sync {
            dir: Some("/notes".into()),
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::Sync { dir } => assert_eq!(dir, Some("/notes".into())),
            other => panic!("expected Sync, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_maintain() {
        let cmd = Commands::Maintain {
            scope: "lint".into(),
        };
        let op = command_to_operation(cmd).unwrap();
        match op {
            Operation::Maintain { scope } => assert_eq!(scope, Some("lint".into())),
            other => panic!("expected Maintain, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_command_to_operation_serve_error() {
        let cmd = Commands::Serve {
            transport: "stdio".into(),
            port: 3000,
        };
        let result = command_to_operation(cmd);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("serve is handled separately"));
    }
}
