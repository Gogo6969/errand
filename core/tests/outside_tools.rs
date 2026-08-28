//! Do the tools people already configured actually reach us?
//!
//! Errand reads the servers out of `~/.claude.json` rather than keeping a list
//! of its own, so this is really asking two questions at once: whether the file
//! is read the way Claude Code writes it, and whether a real server started
//! from that entry answers. Neither can be settled by a unit test, because both
//! are claims about somebody's machine.
//!
//!     cargo test -p errand-core --test outside_tools -- --ignored --nocapture
//!
//! Ignored by default: it starts real programs, some of them `npx`, and what it
//! finds depends entirely on what the person running it has set up.

use errand_core::mcp;

#[tokio::test(flavor = "multi_thread")]
#[ignore = "starts real MCP servers; run with --ignored"]
async fn the_servers_already_configured_on_this_machine_offer_their_tools_here_too() {
    let here = std::env::current_dir().expect("somewhere to be");
    let want = mcp::configured(&here);
    println!("configured: {}", want.len());
    for server in &want {
        // Never the environment. That is where people keep their keys.
        let how = match &server.how {
            mcp::How::Program { command, args, .. } => format!("{command} {}", args.join(" ")),
            mcp::How::Remote { kind, url } => format!("{kind} {url}"),
        };
        println!("  {} ({}) via {}", server.name, server.from, how);
    }
    assert!(
        !want.is_empty(),
        "no MCP servers configured; nothing for this to check"
    );

    let servers = mcp::Servers::open(&here).await;
    for (name, why) in &servers.trouble {
        println!("  did not start: {name}: {why}");
    }
    for tool in servers.tools() {
        println!(
            "  tool: {} -- {}",
            tool.called,
            first_line(&tool.description)
        );
    }

    assert!(
        !servers.tools().is_empty(),
        "servers were configured but none of them offered a tool"
    );

    // Every name is the one Claude Code would use for the same tool, which is
    // the whole reason for reading its file: the two engines have to be talking
    // about the same thing.
    for tool in servers.tools() {
        assert!(
            tool.called.starts_with("mcp__"),
            "{} is not named the way the other engine names it",
            tool.called
        );
        assert!(tool.called.contains(&tool.server));
    }
}

fn first_line(s: &str) -> String {
    s.lines().next().unwrap_or("").chars().take(70).collect()
}
