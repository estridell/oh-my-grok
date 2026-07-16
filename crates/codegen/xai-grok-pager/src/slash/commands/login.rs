//! `/login` -- log in or re-authenticate with your account.

use crate::app::actions::Action;
use agent_client_protocol as acp;

use crate::slash::command::{AppCtx, ArgItem, CommandExecCtx, CommandResult, SlashCommand};

pub struct LoginCommand;

impl SlashCommand for LoginCommand {
    fn name(&self) -> &str {
        "login"
    }

    fn description(&self) -> &str {
        "Log in or re-authenticate with your account"
    }

    fn usage(&self) -> &str {
        "/login [xai|chatgpt]"
    }

    fn takes_args(&self) -> bool {
        true
    }

    fn arg_placeholder(&self) -> Option<&str> {
        Some("[provider]")
    }

    fn suggest_args(&self, _ctx: &AppCtx, _args_query: &str) -> Option<Vec<ArgItem>> {
        Some(vec![
            ArgItem {
                display: "X / Grok".to_string(),
                match_text: "x xai grok".to_string(),
                insert_text: "xai".to_string(),
                description: "Use the stock X account login flow".to_string(),
            },
            ArgItem {
                display: "ChatGPT".to_string(),
                match_text: "chatgpt openai codex".to_string(),
                insert_text: "chatgpt".to_string(),
                description: "Use ChatGPT OAuth for GPT-5.6 models".to_string(),
            },
        ])
    }

    fn run(&self, _ctx: &mut CommandExecCtx, args: &str) -> CommandResult {
        match args.trim().to_ascii_lowercase().as_str() {
            "" | "x" | "xai" | "grok" => CommandResult::Action(Action::LoginWithMethod(
                acp::AuthMethodId::new(xai_grok_shell::agent::auth_method::GROK_COM_METHOD_ID),
            )),
            "chatgpt" | "openai" | "codex" | "openai-codex" => {
                CommandResult::Action(Action::LoginWithMethod(acp::AuthMethodId::new(
                    xai_grok_shell::auth::chatgpt::AUTH_METHOD_ID,
                )))
            }
            provider => CommandResult::Error(format!(
                "Unknown login provider '{provider}'. Use xai or chatgpt."
            )),
        }
    }
}
