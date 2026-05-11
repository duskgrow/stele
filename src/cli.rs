use std::collections::BTreeMap;
use std::sync::Arc;

use clap::{Arg, Command};
use serde_json::json;

use crate::ops::OperationRegistry;
use crate::ops::handler::OpHandler;
use crate::types::Result;

/// Build the CLI command tree dynamically from inventory.
pub(crate) fn build_cli() -> Command {
    let mut groups: BTreeMap<&'static str, Vec<&'static dyn OpHandler>> = BTreeMap::new();
    let mut top_level: Vec<&'static dyn OpHandler> = Vec::new();

    for handler in inventory::iter::<&'static dyn OpHandler>.into_iter() {
        let name = handler.name();
        if let Some((prefix, _suffix)) = name.split_once('.') {
            groups.entry(prefix).or_default().push(*handler);
        } else {
            top_level.push(*handler);
        }
    }

    let mut cmd = Command::new("stele")
        .about("Stele CLI - Personal knowledge management")
        .subcommand(
            Command::new("serve")
                .about("Start the MCP server")
                .arg(Arg::new("transport").long("transport").default_value("stdio"))
                .arg(Arg::new("host").long("host").default_value("127.0.0.1"))
                .arg(Arg::new("port").long("port").value_parser(clap::value_parser!(u16)).default_value("3000"))
        )
        .subcommand(
            Command::new("skill")
                .about("Skill management")
                .subcommand(
                    Command::new("install")
                        .about("Install skills to target directory")
                        .arg(Arg::new("target").long("target").default_value("~/.hermes/skills/"))
                )
        );

    for (prefix, handlers) in &groups {
        let mut parent = Command::new(*prefix).about(format!("{} operations", prefix));
        for handler in handlers {
            parent = parent.subcommand(handler.cli_command());
        }
        cmd = cmd.subcommand(parent);
    }

    for handler in &top_level {
        cmd = cmd.subcommand(handler.cli_command());
    }

    cmd
}

/// Parse CLI arguments and dispatch to the operation registry.
pub async fn run_cli(registry: Arc<OperationRegistry>) -> Result<()> {
    let cmd = build_cli();

    let matches = match cmd.try_get_matches_from(std::env::args()) {
        Ok(m) => m,
        Err(e) => {
            let err_json = json!({"error": e.to_string()});
            eprintln!("{}", serde_json::to_string_pretty(&err_json).unwrap_or_default());
            std::process::exit(1);
        }
    };

    match matches.subcommand() {
        Some(("serve", args)) => {
            let transport = args.get_one::<String>("transport").unwrap();
            let host = args.get_one::<String>("host").unwrap();
            let port = *args.get_one::<u16>("port").unwrap();
            if transport == "stdio" {
                crate::mcp::stdio::run_stdio(registry).await
            } else {
                crate::mcp::http::run_http(registry, host, port).await
            }
        }
        Some(("skill", skill_matches)) => {
            match skill_matches.subcommand() {
                Some(("install", install_matches)) => {
                    let target = install_matches.get_one::<String>("target").unwrap();
                    let target_path = crate::skills::expand_tilde(target)?;
                    crate::skills::install_skills(&target_path)?;
                    let result = json!({"status": "ok", "target": target_path.to_string_lossy()});
                    println!("{}", serde_json::to_string_pretty(&result)?);
                    Ok(())
                }
                _ => {
                    eprintln!("No skill subcommand specified. Use --help for usage.");
                    std::process::exit(1);
                }
            }
        }
        Some((name, sub_matches)) => {
            let result = if let Some(handler) = inventory::iter::<&'static dyn OpHandler>
                .into_iter()
                .find(|h| h.name() == name)
                .copied()
            {
                let op = handler.from_cli_matches(sub_matches)
                    .map_err(|e| crate::types::Error::Config(e.to_string()))?;
                registry.execute_op(op).await
            } else if let Some((sub_name, sub_sub_matches)) = sub_matches.subcommand() {
                let full_name = format!("{}.{}", name, sub_name);
                let handler = inventory::iter::<&'static dyn OpHandler>
                    .into_iter()
                    .find(|h| h.name() == full_name)
                    .copied()
                    .ok_or_else(|| crate::types::Error::Config(format!("unknown op: {}", full_name)))?;
                let op = handler.from_cli_matches(sub_sub_matches)
                    .map_err(|e| crate::types::Error::Config(e.to_string()))?;
                registry.execute_op(op).await
            } else {
                Err(crate::types::Error::Config(format!("unknown command: {}", name)))
            };

            match result {
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
        None => {
            eprintln!("No command specified. Use --help for usage.");
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_parses_serve() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(["stele", "serve", "--transport", "http", "--port", "8080"]).unwrap();
        match matches.subcommand() {
            Some(("serve", args)) => {
                assert_eq!(args.get_one::<String>("transport").unwrap(), "http");
                assert_eq!(*args.get_one::<u16>("port").unwrap(), 8080);
            }
            other => panic!("expected serve, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_serve_defaults() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(["stele", "serve"]).unwrap();
        match matches.subcommand() {
            Some(("serve", args)) => {
                assert_eq!(args.get_one::<String>("transport").unwrap(), "stdio");
                assert_eq!(*args.get_one::<u16>("port").unwrap(), 3000);
            }
            other => panic!("expected serve, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_page_get() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(["stele", "page", "get", "hello-world"]).unwrap();
        match matches.subcommand() {
            Some(("page", page_matches)) => {
                match page_matches.subcommand() {
                    Some(("get", get_matches)) => {
                        assert_eq!(get_matches.get_one::<String>("slug").unwrap(), "hello-world");
                    }
                    other => panic!("expected get, got {:?}", other),
                }
            }
            other => panic!("expected page, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_help() {
        let cmd = build_cli();
        let result = cmd.try_get_matches_from(["stele", "--help"]);
        assert!(result.is_err());
        let err = result.unwrap_err();
        let msg = err.to_string();
        assert!(msg.contains("Stele CLI"), "help should mention 'Stele CLI': {}", msg);
    }

    #[test]
    fn test_cli_parses_skill_install() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(["stele", "skill", "install", "--target", "/tmp/test"]).unwrap();
        match matches.subcommand() {
            Some(("skill", skill_matches)) => {
                match skill_matches.subcommand() {
                    Some(("install", install_matches)) => {
                        assert_eq!(install_matches.get_one::<String>("target").unwrap(), "/tmp/test");
                    }
                    other => panic!("expected install, got {:?}", other),
                }
            }
            other => panic!("expected skill, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_parses_skill_install_defaults() {
        let cmd = build_cli();
        let matches = cmd.try_get_matches_from(["stele", "skill", "install"]).unwrap();
        match matches.subcommand() {
            Some(("skill", skill_matches)) => {
                match skill_matches.subcommand() {
                    Some(("install", install_matches)) => {
                        assert_eq!(install_matches.get_one::<String>("target").unwrap(), "~/.hermes/skills/");
                    }
                    other => panic!("expected install, got {:?}", other),
                }
            }
            other => panic!("expected skill, got {:?}", other),
        }
    }

    #[test]
    fn test_cli_help_shows_all_ops() {
        let mut cmd = build_cli();
        let mut buf = Vec::new();
        cmd.write_help(&mut buf).unwrap();
        let help = String::from_utf8(buf).unwrap();

        assert!(help.contains("serve"), "help should contain 'serve'");
        assert!(help.contains("skill"), "help should contain 'skill'");
        assert!(help.contains("page"), "help should contain 'page'");
        assert!(help.contains("search"), "help should contain 'search'");
        assert!(help.contains("stats"), "help should contain 'stats'");
        assert!(help.contains("reindex"), "help should contain 'reindex'");
        assert!(help.contains("sync"), "help should contain 'sync'");
        assert!(help.contains("maintain"), "help should contain 'maintain'");
        assert!(help.contains("graph"), "help should contain 'graph'");

        let page_sub = cmd.find_subcommand("page").unwrap().clone();
        let mut page_buf = Vec::new();
        page_sub.clone().write_help(&mut page_buf).unwrap();
        let page_help = String::from_utf8(page_buf).unwrap();
        assert!(page_help.contains("get"), "page help should contain 'get'");
        assert!(page_help.contains("put"), "page help should contain 'put'");
        assert!(page_help.contains("delete"), "page help should contain 'delete'");
        assert!(page_help.contains("list"), "page help should contain 'list'");
    }
}
