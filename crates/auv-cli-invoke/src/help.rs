use crate::{CommandGroup, CommandNode, InvokeCommand, InvokeRegistry};

pub fn render_help_index(registry: &InvokeRegistry) -> String {
  let mut commands = Vec::new();
  for group in registry.groups() {
    collect_commands(group, &mut commands);
  }
  let command_width = commands.iter().map(|command| command.id.len()).chain(["help".len()]).max().unwrap_or(0) + 2;

  let mut help = String::from(
    "Invoke typed computer-use operations through AUV's shared local and remote execution model.\n\nEach invocation creates or joins a Run, calls one registered operation, and records its result and artifacts through the same execution path used by other frontends.\n\nExamples:\n  # List displays on the local Device\n  auv invoke display.list\n\n  # Run the same operation on a paired Device\n  auv --device node1 invoke display.list\n\n  # Inspect command-specific arguments and examples\n  auv invoke screen.findText --help\n\nUsage:\n  auv invoke <COMMAND> [OPTIONS]\n\nCommands:\n",
  );
  for command in commands {
    help.push_str("  ");
    help.push_str(&format!("{:<command_width$}{}\n", command.id, command.description));
  }
  help.push_str(&format!("  {:<command_width$}Print help for invoke or one command\n", "help"));
  help.push_str(
    "\nOptions:\n  --target <TARGET>    Select a command-supported target; see command help for accepted types\n  --dry-run            Validate without performing the operation\n  --store-root <PATH>  Persist the recorded run under this directory\n  --no-overlay         Disable live visual overlay presentation\n  --json               Render machine-readable JSON output\n  --compact-json       Render single-line JSON, preserving all fields\n  --detail             Include diagnostic detail in human output\n  --wide               Include extra columns in human table output\n\nUse \"auv invoke <COMMAND> --help\" for command-specific options.\n",
  );

  help
}

fn collect_commands<'a>(group: &'a CommandGroup, commands: &mut Vec<&'a InvokeCommand>) {
  for child in &group.children {
    match child {
      CommandNode::Command(command) => commands.push(command),
      CommandNode::Group(group) => collect_commands(group, commands),
    }
  }
}

pub fn render_command_help(command: &InvokeCommand) -> String {
  let mut clap_command = crate::command::with_invoke_context(command.clap_command(), command.target);
  // Use Clap's inline layout, retaining the expanded documentation rather than
  // dropping examples or target/platform constraints for the sake of brevity.
  if let Some(about) = clap_command.get_long_about().cloned() {
    clap_command = clap_command.about(about).long_about(None::<&str>);
  }
  if let Some(help) = clap_command.get_before_long_help().cloned() {
    clap_command = clap_command.before_help(help).before_long_help(None::<&str>);
  }
  if let Some(help) = clap_command.get_after_long_help().cloned() {
    clap_command = clap_command.after_help(help).after_long_help(None::<&str>);
  }
  clap_command = clap_command.mut_args(|arg| {
    if let Some(help) = arg.get_long_help().cloned() {
      arg.help(help).long_help(None::<&str>)
    } else {
      arg
    }
  });
  clap_command.render_help().to_string()
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn command_help_uses_compact_rows_without_losing_long_documentation() {
    for command in crate::default_registry().all() {
      let mut expanded = crate::command::with_invoke_context(command.clap_command(), command.target);
      let expanded = expanded.render_long_help().to_string();
      let compact = render_command_help(command);
      // Ignore Clap's generated short/long-help hint; compare every contract word,
      // including examples, defaults, possible values and platform constraints.
      let words = |text: &str| {
        text
          .lines()
          .filter(|line| !line.contains("Print help") && !line.trim_start().starts_with("-h, --help"))
          .flat_map(str::split_whitespace)
          .map(str::to_owned)
          .collect::<Vec<_>>()
      };
      assert_eq!(words(&compact), words(&expanded), "{} lost help content", command.id);
      assert!(compact.lines().count() < expanded.lines().count(), "{} still uses expanded rows", command.id);
    }
  }
}
