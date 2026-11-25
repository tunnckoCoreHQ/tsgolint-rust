//! `no-unsafe-class-instance-assignment`: disallows giving a value that was
//! not created by a class constructor to a place that expects an instance of
//! that class.

use goport_core::ast::{
    is_array_literal_expression, is_assignment_target, is_block, is_import_call,
    is_object_literal_expression, is_omitted_expression, is_spread_element, skip_parentheses,
};
use goport_core::astdata::SyntaxKind;
use goport_core::checker::Checker;
use goport_core::core::{Node, TypeId};
use goport_core::flags::{ContextFlags, ObjectFlags, TypeFlags};
use rustc_hash::FxHashSet;
use serde_json::Value;
use tsgolint_core::{Listeners, RuleContext, RuleDefinition, RuleListeners, RuleMessage};

use crate::utils::{
    intersection_type_parts, is_object_type, is_type_flag_set, matches_type_or_base_type,
};

pub static RULE: RuleDefinition = RuleDefinition {
    name: "no-unsafe-class-instance-assignment",
    create,
};

fn non_instance_message(type_name: &str) -> RuleMessage {
    RuleMessage::new(
        "nonInstance",
        format!(
            "This value is used where an instance of class `{type_name}` is expected, but was not created by its constructor."
        ),
    )
}

fn object_literal_message(type_name: &str) -> RuleMessage {
    RuleMessage::new(
        "objectLiteral",
        format!(
            "This object literal is used where an instance of class `{type_name}` is expected."
        ),
    )
}

fn create(_ctx: &mut RuleContext<'_>, _options: &Value) -> RuleListeners {
    Listeners::new(())
        .on(SyntaxKind::ArrayLiteralExpression, |_, ctx, node| {
            if is_assignment_target(node) {
                return;
            }
            for element in node.elements().iter() {
                if !is_omitted_expression(element) && !is_spread_element(element) {
                    check_site(ctx, element);
                }
            }
        })
        .on(SyntaxKind::ArrowFunction, |_, ctx, node| {
            let body = node.body();
            if !is_block(body) {
                check_site(ctx, body);
            }
        })
        .on(SyntaxKind::BinaryExpression, |_, ctx, node| {
            match node.operator_token().kind() {
                SyntaxKind::EqualsToken
                | SyntaxKind::AmpersandAmpersandEqualsToken
                | SyntaxKind::QuestionQuestionEqualsToken
                | SyntaxKind::BarBarEqualsToken
                | SyntaxKind::CommaToken => check_site(ctx, node.right()),
                SyntaxKind::AmpersandAmpersandToken
                | SyntaxKind::BarBarToken
                | SyntaxKind::QuestionQuestionToken => {
                    check_site(ctx, node.left());
                    check_site(ctx, node.right());
                }
                _ => {}
            }
        })
        .on(SyntaxKind::BindingElement, check_initializer)
        .on(SyntaxKind::CallExpression, |_, ctx, node| {
            if !is_import_call(node) {
                check_arguments(ctx, node);
            }
        })
        .on(SyntaxKind::ConditionalExpression, |_, ctx, node| {
            check_site(ctx, node.when_true());
            check_site(ctx, node.when_false());
        })
        .on(SyntaxKind::JsxExpression, |_, ctx, node| {
            if !node.dot_dot_dot_token().is_some() {
                check_site(ctx, node.expression());
            }
        })
        .on(SyntaxKind::NewExpression, |_, ctx, node| {
            check_arguments(ctx, node)
        })
        .on(SyntaxKind::Parameter, check_initializer)
        .on(SyntaxKind::PropertyAssignment, |state, ctx, node| {
            if !is_assignment_target(node.parent()) {
                check_initializer(state, ctx, node);
            }
        })
        .on(SyntaxKind::PropertyDeclaration, check_initializer)
        .on(SyntaxKind::ReturnStatement, check_expression)
        .on(SyntaxKind::ShorthandPropertyAssignment, |_, ctx, node| {
            let initializer = node.object_assignment_initializer();
            if initializer.is_some() {
                check_site(ctx, initializer);
            } else if !is_assignment_target(node.parent()) {
                check_site(ctx, node.name());
            }
        })
        .on(SyntaxKind::AsExpression, check_expression)
        .on(SyntaxKind::SatisfiesExpression, check_expression)
        .on(SyntaxKind::TypeAssertionExpression, check_expression)
        .on(SyntaxKind::VariableDeclaration, check_initializer)
        .on(SyntaxKind::YieldExpression, check_expression)
        .into()
}

fn check_initializer(_: &mut (), ctx: &mut RuleContext<'_>, node: Node) {
    check_site(ctx, node.initializer());
}

fn check_expression(_: &mut (), ctx: &mut RuleContext<'_>, node: Node) {
    check_site(ctx, node.expression());
}

fn check_arguments(ctx: &mut RuleContext<'_>, node: Node) {
    for argument in node.arguments().iter() {
        if !is_spread_element(argument) {
            check_site(ctx, argument);
        }
    }
}

fn is_skipped_site(node: Node) -> bool {
    match node.kind() {
        SyntaxKind::ArrowFunction
        | SyntaxKind::ConditionalExpression
        | SyntaxKind::FunctionExpression => true,
        SyntaxKind::BinaryExpression => matches!(
            node.operator_token().kind(),
            SyntaxKind::AmpersandAmpersandToken
                | SyntaxKind::BarBarToken
                | SyntaxKind::QuestionQuestionToken
                | SyntaxKind::CommaToken
        ),
        _ => false,
    }
}

fn check_site(ctx: &mut RuleContext<'_>, node: Node) {
    if !node.is_some() {
        return;
    }
    let node = skip_parentheses(node);
    if is_skipped_site(node) {
        return;
    }
    let c = &mut *ctx.checker;
    let contextual_type = c.get_contextual_type(node, ContextFlags::NONE);
    if !contextual_type.is_some() {
        return;
    }
    let shallow = is_object_literal_expression(node) || is_array_literal_expression(node);
    let source = c.get_type_at_location(node);
    let violation = find_violation(
        c,
        source,
        contextual_type,
        shallow,
        &mut FxHashSet::default(),
    );
    if !violation.is_some() {
        return;
    }
    let type_name = c.type_to_string(violation);
    let message = if is_object_literal_expression(node) {
        object_literal_message(&type_name)
    } else {
        non_instance_message(&type_name)
    };
    ctx.report_node(node, message);
}

fn is_type_reference(c: &Checker, t: TypeId) -> bool {
    is_object_type(c, t) && c.ty(t).object_flags.intersects(ObjectFlags::REFERENCE)
}

/// The class of a class instance type; nil for other types.
fn class_target(c: &Checker, t: TypeId) -> TypeId {
    if !is_object_type(c, t) {
        return TypeId::NIL;
    }
    let target = if is_type_reference(c, t) {
        c.ty(t).target()
    } else {
        t
    };
    if c.ty(target).object_flags.intersects(ObjectFlags::CLASS) {
        target
    } else {
        TypeId::NIL
    }
}

/// The first of `t` and its base types that is a class instance type.
fn nominal_class(c: &mut Checker, t: TypeId) -> TypeId {
    let mut found = TypeId::NIL;
    matches_type_or_base_type(c, t, |c, candidate| {
        if class_target(c, candidate).is_some() {
            found = candidate;
        }
        found.is_some()
    });
    found
}

fn resolve_constraint(c: &mut Checker, t: TypeId) -> TypeId {
    if is_type_flag_set(c, t, TypeFlags::INSTANTIABLE) {
        c.get_base_constraint_of_type(t)
    } else {
        t
    }
}

fn has_class(c: &mut Checker, t: TypeId) -> bool {
    contains_class(c, t, &mut FxHashSet::default())
}

fn contains_class(c: &mut Checker, t: TypeId, seen: &mut FxHashSet<TypeId>) -> bool {
    if !seen.insert(t) {
        return false;
    }
    let resolved = resolve_constraint(c, t);
    if !resolved.is_some() {
        return false;
    }
    if is_type_flag_set(c, resolved, TypeFlags::UNION_OR_INTERSECTION) {
        let parts = c.ty(resolved).types_list();
        return parts.into_iter().any(|part| contains_class(c, part, seen));
    }
    if nominal_class(c, resolved).is_some() {
        return true;
    }
    if is_type_reference(c, resolved) {
        let arguments = c.get_type_arguments(resolved);
        if arguments
            .into_iter()
            .any(|argument| contains_class(c, argument, seen))
        {
            return true;
        }
    }
    let infos = c.get_index_infos_of_type(resolved);
    infos.into_iter().any(|info| {
        let value_type = c.index_info(info).value_type();
        contains_class(c, value_type, seen)
    })
}

fn is_instance_of(c: &mut Checker, source: TypeId, class_type: TypeId) -> bool {
    let target = class_target(c, class_type);
    intersection_type_parts(c, source).into_iter().any(|part| {
        if is_object_type(c, part) && c.ty(part).object_flags.intersects(ObjectFlags::MAPPED) {
            return true;
        }
        matches_type_or_base_type(c, part, |c, t| class_target(c, t) == target)
    })
}

/// The class whose instance `source` is given in place of, nil when none.
/// `active` holds the comparisons in progress, which end a cycle.
fn find_violation(
    c: &mut Checker,
    source: TypeId,
    target: TypeId,
    shallow: bool,
    active: &mut FxHashSet<(TypeId, TypeId, bool)>,
) -> TypeId {
    let source = resolve_constraint(c, source);
    let target = resolve_constraint(c, target);
    if !source.is_some()
        || !target.is_some()
        || source == target
        || is_type_flag_set(
            c,
            source,
            TypeFlags::ANY | TypeFlags::UNKNOWN | TypeFlags::NEVER,
        )
        || !has_class(c, target)
    {
        return TypeId::NIL;
    }
    let comparison = (source, target, shallow);
    if !active.insert(comparison) {
        return TypeId::NIL;
    }
    let violation = find_violation_worker(c, source, target, shallow, active);
    active.remove(&comparison);
    violation
}

fn find_violation_worker(
    c: &mut Checker,
    source: TypeId,
    target: TypeId,
    shallow: bool,
    active: &mut FxHashSet<(TypeId, TypeId, bool)>,
) -> TypeId {
    if is_type_flag_set(c, source, TypeFlags::UNION) {
        let parts = c.ty(source).types_list();
        for part in parts {
            if !c.is_type_assignable_to(part, target) {
                continue;
            }
            let violation = find_violation(c, part, target, shallow, active);
            if violation.is_some() {
                return violation;
            }
        }
        return TypeId::NIL;
    }

    if !is_object_type(c, source) {
        let never = c.get_never_type();
        if !is_type_flag_set(c, source, TypeFlags::INTERSECTION)
            || c.is_type_assignable_to(source, never)
        {
            return TypeId::NIL;
        }
    }

    if is_type_flag_set(c, target, TypeFlags::UNION) {
        let parts = c.ty(target).types_list();
        let mut candidates = Vec::new();
        for part in parts {
            if c.is_type_assignable_to(source, part) {
                candidates.push(part);
            }
        }
        if candidates.is_empty() {
            return TypeId::NIL;
        }
        for &candidate in &candidates {
            if !has_class(c, candidate) {
                return TypeId::NIL;
            }
        }
        let mut first_violation = TypeId::NIL;
        for candidate in candidates {
            let violation = find_violation(c, source, candidate, shallow, active);
            if !violation.is_some() {
                return TypeId::NIL;
            }
            if !first_violation.is_some() {
                first_violation = violation;
            }
        }
        return first_violation;
    }

    if is_type_flag_set(c, target, TypeFlags::INTERSECTION) {
        let parts = c.ty(target).types_list();
        for part in parts {
            let violation = find_violation(c, source, part, shallow, active);
            if violation.is_some() {
                return violation;
            }
        }
        return TypeId::NIL;
    }

    let nominal = nominal_class(c, target);
    if nominal.is_some() && !is_instance_of(c, source, nominal) {
        return nominal;
    }

    if shallow {
        return TypeId::NIL;
    }

    if is_type_reference(c, source)
        && is_type_reference(c, target)
        && c.ty(source).target() == c.ty(target).target()
    {
        let source_arguments = c.get_type_arguments(source);
        let target_arguments = c.get_type_arguments(target);
        for (&source_argument, &target_argument) in
            source_arguments.iter().zip(target_arguments.iter())
        {
            let violation = find_violation(c, source_argument, target_argument, false, active);
            if violation.is_some() {
                return violation;
            }
        }
        return TypeId::NIL;
    }

    for key_type in [c.get_number_type(), c.get_string_type()] {
        let source_info = c.get_index_info_of_type(source, key_type);
        let target_info = c.get_index_info_of_type(target, key_type);
        if !source_info.is_some() || !target_info.is_some() {
            continue;
        }
        let source_value = c.index_info(source_info).value_type();
        let target_value = c.index_info(target_info).value_type();
        let violation = find_violation(c, source_value, target_value, false, active);
        if violation.is_some() {
            return violation;
        }
    }
    TypeId::NIL
}
