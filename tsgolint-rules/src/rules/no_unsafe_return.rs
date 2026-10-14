//! `no-unsafe-return`: disallows returning an `any` typed value from a
//! function.

use goport_core::ast::{
    has_syntactic_modifier, is_arrow_function, is_block, is_function_expression,
};
use goport_core::astdata::SyntaxKind;
use goport_core::checker::Checker;
use goport_core::core::{Node, TypeId};
use goport_core::flags::{ModifierFlags, TypeFlags};
use serde_json::Value;
use tsgolint_core::diagnostic::TextRange;
use tsgolint_core::{Listeners, Report, RuleContext, RuleDefinition, RuleListeners, RuleMessage};

use crate::utils::{
    DiscriminatedAnyType, any_typed_this_expression, collect_all_call_signatures,
    discriminate_any_type, get_constrained_type_at_location, get_contextual_type,
    get_parent_function_node, is_intrinsic_error_type, is_type_flag_set,
    is_type_unknown_array_type, is_type_unknown_type, is_unsafe_assignment,
    type_to_string_or_error,
};

pub static RULE: RuleDefinition = RuleDefinition {
    name: "no-unsafe-return",
    create,
};

fn unsafe_return_message(type_name: &str) -> RuleMessage {
    RuleMessage::new(
        "unsafeReturn",
        format!("Unsafe return of a value of type {type_name}."),
    )
}

fn unsafe_return_assignment_message(sender: &str, receiver: &str) -> RuleMessage {
    RuleMessage::new(
        "unsafeReturnAssignment",
        format!("Unsafe return of type `{sender}` from function with return type `{receiver}`."),
    )
}

fn unsafe_return_this_message(type_name: &str) -> RuleMessage {
    RuleMessage::new(
        "unsafeReturnThis",
        format!("Unsafe return of a value of type {type_name}. `this` is typed as `any`."),
    )
    .with_help("Enable `noImplicitThis` or add an explicit `this` parameter to the function.")
}

struct State {
    no_implicit_this: bool,
}

fn create(ctx: &mut RuleContext<'_>, _options: &Value) -> RuleListeners {
    let options = ctx.compiler_options();
    let no_implicit_this = options.get_strict_option_value(options.no_implicit_this);
    Listeners::new(State { no_implicit_this })
        .on(SyntaxKind::ArrowFunction, |state, ctx, node| {
            let body = node.body();
            if !is_block(body) {
                check_return(state, ctx, body, body);
            }
        })
        .on(SyntaxKind::ReturnStatement, |state, ctx, node| {
            let argument = node.expression();
            if argument.is_some() {
                check_return(state, ctx, argument, node);
            }
        })
        .into()
}

fn is_any_or_unknown(checker: &Checker, t: TypeId) -> bool {
    is_type_flag_set(checker, t, TypeFlags::ANY_OR_UNKNOWN)
}

/// The report of a return, with the returned type and, when the function has
/// a declared or contextual return type, that type in its help.
fn return_report(
    range: TextRange,
    returned_type: &str,
    expected_type: &str,
    mut message: RuleMessage,
) -> Report {
    let mut help = if expected_type.is_empty() {
        format!("Returned expression has type `{returned_type}`.")
    } else {
        format!(
            "Returned expression has type `{returned_type}`; the function return type is `{expected_type}`."
        )
    };
    if message.help.is_empty() {
        help.push_str(" Return a safely typed value or narrow the value before returning it.");
    } else {
        help.push(' ');
        help.push_str(&message.help);
    }
    message.help = help;
    Report::new(range, message)
}

fn check_return(state: &State, ctx: &mut RuleContext<'_>, return_node: Node, reporting_node: Node) {
    let function_node = get_parent_function_node(return_node);
    if function_node.is_nil() {
        return;
    }
    let return_node_type = ctx.checker.get_type_at_location(return_node);
    let any_type = discriminate_any_type(ctx.checker, return_node_type, return_node);

    let constrained_return_node_type = get_constrained_type_at_location(ctx.checker, return_node);

    // A function expression keeps its own return type even when the receiver
    // is typed (`const f: () => Set<string> = () => new Set<any>()`), so use
    // the contextual type for those.
    let mut function_type = TypeId::NIL;
    if is_function_expression(function_node) || is_arrow_function(function_node) {
        function_type = get_contextual_type(ctx.checker, function_node);
    }
    let uses_contextual_type = function_type.is_some();
    if function_type.is_nil() {
        function_type = ctx.checker.get_type_at_location(function_node);
    }
    let call_signatures = collect_all_call_signatures(ctx.checker, function_type);
    let return_type_node = function_node.type_();

    let report = |ctx: &mut RuleContext<'_>, message: RuleMessage| {
        let mut expected_type = String::new();
        if (return_type_node.is_some() || uses_contextual_type)
            && let Some(&signature) = call_signatures.first()
        {
            let return_type = ctx.checker.get_return_type_of_signature(signature);
            expected_type = type_to_string_or_error(ctx.checker, return_type);
        }
        let returned_type = type_to_string_or_error(ctx.checker, constrained_return_node_type);
        let report = return_report(
            ctx.trim_node_text_range(reporting_node),
            &returned_type,
            &expected_type,
            message,
        );
        ctx.report_diagnostic(report);
    };

    let is_async = has_syntactic_modifier(function_node, ModifierFlags::ASYNC);

    // An explicit return type that matches the returned type is intentional,
    // even when unsafe.
    if return_type_node.is_some() {
        for &signature in &call_signatures {
            let signature_return_type = ctx.checker.get_return_type_of_signature(signature);
            if return_node_type == signature_return_type
                || is_any_or_unknown(ctx.checker, signature_return_type)
            {
                return;
            }
            if is_async {
                let awaited_signature_return_type =
                    ctx.checker.get_awaited_type(signature_return_type);
                let awaited_return_node_type = ctx.checker.get_awaited_type(return_node_type);
                if awaited_signature_return_type == awaited_return_node_type
                    || (awaited_signature_return_type.is_some()
                        && is_any_or_unknown(ctx.checker, awaited_signature_return_type))
                {
                    return;
                }
            }
        }
    }

    if any_type != DiscriminatedAnyType::Safe {
        // Returning `any` or `any[]` from a function declared to return
        // `unknown` or `unknown[]` is allowed.
        for &signature in &call_signatures {
            let function_return_type = ctx.checker.get_return_type_of_signature(signature);
            if any_type == DiscriminatedAnyType::Any
                && is_type_unknown_type(ctx.checker, function_return_type)
            {
                return;
            }
            if any_type == DiscriminatedAnyType::AnyArray
                && is_type_unknown_array_type(ctx.checker, function_return_type)
            {
                return;
            }
            let awaited_type = ctx.checker.get_awaited_type(function_return_type);
            if awaited_type.is_some()
                && any_type == DiscriminatedAnyType::PromiseAny
                && is_type_unknown_type(ctx.checker, awaited_type)
            {
                return;
            }
        }

        if any_type == DiscriminatedAnyType::PromiseAny && !is_async {
            return;
        }

        let type_string = if is_intrinsic_error_type(ctx.checker, constrained_return_node_type) {
            "error"
        } else {
            match any_type {
                DiscriminatedAnyType::Any => "`any`",
                DiscriminatedAnyType::PromiseAny => "`Promise<any>`",
                _ => "`any[]`",
            }
        };

        // `return this`
        if !state.no_implicit_this && any_typed_this_expression(ctx.checker, return_node).is_some()
        {
            report(ctx, unsafe_return_this_message(type_string));
            return;
        }
        report(ctx, unsafe_return_message(type_string));
        return;
    }

    let Some(&signature) = call_signatures.first() else {
        return;
    };
    let function_return_type = ctx.checker.get_return_type_of_signature(signature);
    if let Some(unsafe_assignment) = is_unsafe_assignment(
        ctx.checker,
        return_node_type,
        function_return_type,
        return_node,
    ) {
        let sender = ctx.checker.type_to_string(unsafe_assignment.sender);
        let receiver = ctx.checker.type_to_string(unsafe_assignment.receiver);
        report(ctx, unsafe_return_assignment_message(&sender, &receiver));
    }
}
