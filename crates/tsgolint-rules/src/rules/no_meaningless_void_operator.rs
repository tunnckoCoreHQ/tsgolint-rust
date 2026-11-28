//! `no-meaningless-void-operator`: disallows `void` on an expression that is
//! already `void` (or `never`, with `checkNever`), and on anything but a call.

use goport_core::ast::{
    is_assignment_expression, is_call_expression, is_expression_statement, is_import_call,
    is_numeric_literal, is_parenthesized_expression, is_string_literal, skip_outer_expressions,
    source_file_text,
};
use goport_core::astdata::SyntaxKind;
use goport_core::core::Node;
use goport_core::flags::{OuterExpressionKinds, TypeFlags};
use goport_core::frontend::scanner::scanner_ls::get_scanner_for_source_file;
use serde::Deserialize;
use serde_json::Value;
use tsgolint_core::{
    Listeners, RuleContext, RuleDefinition, RuleFix, RuleListeners, RuleMessage, RuleSuggestion,
    decode_options,
};

use crate::utils::{
    is_start_of_expression_statement_needing_parentheses, is_thenable_type, trim_node_text_range,
    union_type_parts,
};

pub static RULE: RuleDefinition = RuleDefinition {
    name: "no-meaningless-void-operator",
    create,
};

// The schema.json defaults are the `Default` impl. Missing and `null`
// properties take them.
#[derive(Deserialize, Default)]
#[serde(default, rename_all = "camelCase")]
struct Options {
    check_never: bool,
}

fn meaningless_void_operator_message(type_name: &str) -> RuleMessage {
    RuleMessage::new(
        "meaninglessVoidOperator",
        format!(
            "void operator shouldn't be used on {type_name}; \
             it should convey that a return value is being ignored"
        ),
    )
}

fn remove_void_message() -> RuleMessage {
    RuleMessage::new("removeVoid", "Remove 'void'")
}

fn create(_ctx: &mut RuleContext<'_>, options: &Value) -> RuleListeners {
    let options: Options = decode_options(options, RULE.name);
    Listeners::new(options)
        .on(SyntaxKind::VoidExpression, check_void)
        .into()
}

fn meaningless_void_on_non_call_message() -> RuleMessage {
    RuleMessage::new(
        "meaninglessVoidOnNonCall",
        "void operator is useless here; it should only discard a call's return value",
    )
}

/// Removes `void` and keeps an expression statement parsed as one: wraps the
/// operand in parentheses when it would start a declaration or directive,
/// and adds `;` when it would continue the previous statement.
fn remove_void_keyword(source_file: Node, node: Node, argument: Node) -> Vec<RuleFix> {
    let keyword = trim_node_text_range(source_file, node).with_end(argument.pos());
    let statement = node.parent();
    if !is_expression_statement(statement) {
        return vec![RuleFix::remove_range(keyword)];
    }

    let text = source_file_text(source_file);
    let mut s = get_scanner_for_source_file(source_file, &text, argument.pos());
    let mut first_token = s.token();
    if first_token == SyntaxKind::AsyncKeyword
        && s.scan() == SyntaxKind::FunctionKeyword
        && !s.has_preceding_line_break()
    {
        first_token = SyntaxKind::FunctionKeyword;
    }
    let wrap = is_string_literal(argument)
        || first_token == SyntaxKind::AtToken
        || (first_token == SyntaxKind::LetKeyword && s.scan() == SyntaxKind::OpenBracketToken)
        || is_start_of_expression_statement_needing_parentheses(source_file, node, first_token);
    if wrap {
        first_token = SyntaxKind::OpenParenToken;
    }

    let prefix = if matches!(
        first_token,
        SyntaxKind::OpenParenToken
            | SyntaxKind::OpenBracketToken
            | SyntaxKind::PlusToken
            | SyntaxKind::MinusToken
            | SyntaxKind::SlashToken
            | SyntaxKind::SlashEqualsToken
            | SyntaxKind::LessThanToken
            | SyntaxKind::NoSubstitutionTemplateLiteral
            | SyntaxKind::TemplateHead
    ) && previous_statement_lacks_semicolon(&text, statement)
    {
        ";"
    } else {
        ""
    };
    if wrap {
        return vec![
            RuleFix::replace_range(keyword, format!("{prefix}(")),
            RuleFix::insert_after(node, ")"),
        ];
    }
    vec![RuleFix::replace_range(keyword, prefix)]
}

fn previous_statement_lacks_semicolon(text: &str, statement: Node) -> bool {
    let parent = statement.parent();
    if !parent.can_have_statements() {
        return false;
    }
    let statements = parent.statements();
    let index = statements
        .iter()
        .position(|s| s.pos() >= statement.pos())
        .unwrap_or(statements.len());
    index > 0 && !text[..statements.get(index - 1).end() as usize].ends_with(';')
}

fn check_void(options: &mut Options, ctx: &mut RuleContext<'_>, node: Node) {
    let argument = node.expression();
    let inner = skip_outer_expressions(
        argument,
        OuterExpressionKinds::OEK_PARENTHESES
            | OuterExpressionKinds::OEK_ASSERTIONS
            | OuterExpressionKinds::OEK_COMMA,
    );
    // `void (x = value)` and `void 0` are explicit.
    if is_assignment_expression(inner, false) || (is_numeric_literal(inner) && inner.text() == "0")
    {
        return;
    }
    let argument_type = ctx.checker.get_type_at_location(argument);
    let remove_void =
        |ctx: &mut RuleContext<'_>| remove_void_keyword(ctx.source_file, node, argument);

    if !is_call_expression(inner) || is_import_call(inner) {
        if is_thenable_type(ctx.checker, argument, argument_type) {
            return;
        }
        let mut parent = node.parent();
        while is_parenthesized_expression(parent) {
            parent = parent.parent();
        }
        let message = meaningless_void_on_non_call_message();
        if is_expression_statement(parent) {
            ctx.report_node_with_fixes(node, message, remove_void);
        } else {
            ctx.report_node(node, message);
        }
        return;
    }

    let parts = union_type_parts(ctx.checker, argument_type);
    let all_have = |flags: TypeFlags| {
        parts
            .iter()
            .all(|&t| ctx.checker.ty(t).flags.intersects(flags))
    };
    let always_void_like = all_have(TypeFlags::VOID_LIKE);
    let always_void_like_or_never = all_have(TypeFlags::VOID_LIKE | TypeFlags::NEVER);

    if always_void_like {
        // Compute the message first: it needs `ctx.checker` mutably.
        let message = meaningless_void_operator_message(&ctx.checker.type_to_string(argument_type));
        ctx.report_node_with_fixes(node, message, remove_void);
    } else if options.check_never && always_void_like_or_never {
        let message = meaningless_void_operator_message(&ctx.checker.type_to_string(argument_type));
        ctx.report_node_with_suggestions(node, message, |ctx| {
            vec![RuleSuggestion {
                message: remove_void_message(),
                fixes: remove_void(ctx),
            }]
        });
    }
}
