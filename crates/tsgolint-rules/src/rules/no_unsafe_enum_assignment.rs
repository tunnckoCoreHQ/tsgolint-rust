//! `no-unsafe-enum-assignment`: disallows giving a value that is not of an
//! enum to a place that expects that enum.

use std::collections::BTreeSet;

use goport_core::ast::{
    is_array_literal_expression, is_assignment_expression, is_async_function, is_binary_expression,
    is_block, is_computed_property_name, is_identifier, is_jsx_expression, is_mapped_type_node,
    is_numeric_literal, is_object_literal_expression, is_parameter_declaration,
    is_prefix_unary_expression, is_spread_element, is_static, is_string_literal,
    is_template_expression, is_type_literal_node, is_type_reference_node, skip_parentheses,
};
use goport_core::astdata::SyntaxKind;
use goport_core::checker::{Checker, SharedList};
use goport_core::core::{Node, TypeId};
use goport_core::flags::{CheckMode, ContextFlags, ElementFlags, ObjectFlags, TypeFlags};
use rustc_hash::{FxHashMap, FxHashSet};
use serde_json::Value;
use tsgolint_core::{Listener, Listeners, RuleContext, RuleDefinition, RuleListeners, RuleMessage};

use crate::utils::{
    get_enum_types, get_heritage_clauses, get_number_index_type, get_parent_function_node,
    intersection_type_parts, is_object_type, is_rest_parameter_declaration, is_type_flag_set,
    literal_type_value, union_type_parts,
};

pub static RULE: RuleDefinition = RuleDefinition {
    name: "no-unsafe-enum-assignment",
    create,
};

/// The place that expects the enum.
#[derive(Clone, Copy)]
enum Unsafe {
    Access,
    Argument,
    Assertion,
    Assignment,
    Mutation,
    Return,
}

impl Unsafe {
    /// The message for the expected enums `names`.
    fn message(self, names: &str) -> RuleMessage {
        let (id, description) = match self {
            Unsafe::Access => (
                "unsafeEnumAccess",
                format!(
                    "The computed key used here does not have a shared enum type with the expected enum {names}."
                ),
            ),
            Unsafe::Argument => (
                "unsafeEnumArgument",
                format!(
                    "The argument passed here does not have a shared enum type with the expected enum {names}."
                ),
            ),
            Unsafe::Assertion => (
                "unsafeEnumAssertion",
                format!(
                    "The value asserted here does not have a shared enum type with the expected enum {names}."
                ),
            ),
            Unsafe::Assignment => (
                "unsafeEnumAssignment",
                format!(
                    "The value assigned here does not have a shared enum type with the expected enum {names}."
                ),
            ),
            Unsafe::Mutation => (
                "unsafeEnumMutation",
                format!("This mutation can produce a value outside of the expected enum {names}."),
            ),
            Unsafe::Return => (
                "unsafeEnumReturn",
                format!(
                    "The value returned here does not have a shared enum type with the expected enum {names}."
                ),
            ),
        };
        RuleMessage::new(id, description)
    }
}

#[derive(Default)]
struct State {
    types: EnumTypes,
    /// The expressions that a report covers already.
    checked: FxHashSet<Node>,
}

fn create(_ctx: &mut RuleContext<'_>, _options: &Value) -> RuleListeners {
    Listeners::new(State::default())
        .on(SyntaxKind::VariableDeclaration, check_initializer)
        .on(SyntaxKind::Parameter, check_initializer)
        .on(SyntaxKind::BindingElement, check_initializer)
        .on(
            SyntaxKind::ShorthandPropertyAssignment,
            |state, ctx, node| {
                let initializer = node.object_assignment_initializer();
                if initializer.is_some() {
                    let receiver = ctx.checker.get_type_at_location(node.name());
                    check_assignment(
                        state,
                        ctx,
                        receiver,
                        initializer,
                        node,
                        Unsafe::Assignment,
                        None,
                    );
                }
            },
        )
        .on(SyntaxKind::PropertyDeclaration, check_property_declaration)
        .on(SyntaxKind::BinaryExpression, check_binary_expression)
        .on(SyntaxKind::PrefixUnaryExpression, |_, ctx, node| {
            if matches!(
                node.operator(),
                SyntaxKind::PlusPlusToken | SyntaxKind::MinusMinusToken
            ) {
                check_mutation(ctx, node.operand(), node);
            }
        })
        .on(SyntaxKind::PostfixUnaryExpression, |_, ctx, node| {
            check_mutation(ctx, node.operand(), node);
        })
        .on(SyntaxKind::AsExpression, check_assertion)
        .on(SyntaxKind::TypeAssertionExpression, check_assertion)
        .on(SyntaxKind::SatisfiesExpression, |state, ctx, node| {
            if !state.checked.contains(&node) {
                let receiver = ctx.checker.get_type_at_location(node.type_());
                check_assignment(
                    state,
                    ctx,
                    receiver,
                    node.expression(),
                    node,
                    Unsafe::Assignment,
                    None,
                );
            }
        })
        .on(SyntaxKind::ReturnStatement, |state, ctx, node| {
            let expression = node.expression();
            if expression.is_some() {
                check_return(state, ctx, expression, node);
            }
        })
        .on(SyntaxKind::ArrowFunction, |state, ctx, node| {
            let body = node.body();
            if body.is_some() && !is_block(body) {
                check_return(state, ctx, body, body);
            }
        })
        .on(SyntaxKind::CallExpression, |state, ctx, node| {
            check_arguments(state, ctx, node, node.arguments().iter(), false);
        })
        .on(SyntaxKind::NewExpression, |state, ctx, node| {
            if node.argument_list().is_some() {
                check_arguments(state, ctx, node, node.arguments().iter(), false);
            }
        })
        .on(SyntaxKind::TaggedTemplateExpression, |state, ctx, node| {
            let template = node.template();
            let spans = if is_template_expression(template) {
                template.template_spans().nodes()
            } else {
                Default::default()
            };
            let arguments = spans.iter().map(|span| span.expression());
            check_arguments(state, ctx, node, arguments, true);
        })
        .on(SyntaxKind::JsxAttribute, check_jsx_attribute)
        .on(
            Listener::not_allow_pattern(SyntaxKind::ArrayLiteralExpression),
            check_array_literal,
        )
        .on(
            Listener::not_allow_pattern(SyntaxKind::ObjectLiteralExpression),
            check_object_literal,
        )
        .on(SyntaxKind::ElementAccessExpression, check_element_access)
        .into()
}

fn report(ctx: &mut RuleContext<'_>, node: Node, place: Unsafe, types: &[TypeId]) {
    let names = describe(ctx.checker, types);
    let mut range = ctx.trim_node_text_range(node);
    if is_parameter_declaration(node) {
        // tsgolint starts the range of a parameter at its name.
        range = range.with_pos(ctx.trim_node_text_range(node.name()).pos());
    }
    ctx.report_range(range, place.message(&names));
}

/// Marks `node` and the expressions that give it its value as reported.
fn mark_checked(checked: &mut FxHashSet<Node>, node: Node) {
    let node = skip_parentheses(node);
    checked.insert(node);
    match node.kind() {
        SyntaxKind::ArrayLiteralExpression => {
            for element in node.elements() {
                mark_checked(checked, element);
            }
        }
        SyntaxKind::ObjectLiteralExpression => {
            for property in node.properties() {
                mark_checked(checked, property);
            }
        }
        SyntaxKind::ConditionalExpression => {
            mark_checked(checked, node.when_true());
            mark_checked(checked, node.when_false());
        }
        SyntaxKind::BinaryExpression => {
            if matches!(
                node.operator_token().kind(),
                SyntaxKind::AmpersandAmpersandToken
                    | SyntaxKind::BarBarToken
                    | SyntaxKind::QuestionQuestionToken
            ) {
                mark_checked(checked, node.left());
                mark_checked(checked, node.right());
            }
        }
        SyntaxKind::PropertyAssignment => mark_checked(checked, node.initializer()),
        SyntaxKind::ShorthandPropertyAssignment => mark_checked(checked, node.name()),
        SyntaxKind::SpreadElement
        | SyntaxKind::SpreadAssignment
        | SyntaxKind::AsExpression
        | SyntaxKind::TypeAssertionExpression
        | SyntaxKind::NonNullExpression
        | SyntaxKind::SatisfiesExpression => mark_checked(checked, node.expression()),
        _ => {}
    }
}

/// A `&`, `^`, `|` or `~` expression whose operands all fit `receiver`.
fn safe_bitwise(ctx: &mut RuleContext<'_>, node: Node, receiver: TypeId) -> bool {
    let node = skip_parentheses(node);
    let mut safe_operand = |operand: Node| {
        if safe_bitwise(ctx, operand, receiver) {
            return true;
        }
        let operand_type = ctx.checker.get_type_at_location(operand);
        !direct_mismatch(ctx.checker, operand_type, receiver)
    };
    if is_binary_expression(node)
        && matches!(
            node.operator_token().kind(),
            SyntaxKind::AmpersandToken | SyntaxKind::CaretToken | SyntaxKind::BarToken
        )
    {
        return safe_operand(node.left()) && safe_operand(node.right());
    }
    if is_prefix_unary_expression(node) {
        return node.operator() == SyntaxKind::TildeToken && safe_operand(node.operand());
    }
    false
}

/// Whether the value of `sender_node` does not fit the enums of `receiver`.
/// `sender` is the type of the value when it is not the type of the node.
fn unsafe_assignment(
    state: &mut State,
    ctx: &mut RuleContext<'_>,
    sender_node: Node,
    receiver: TypeId,
    sender: Option<TypeId>,
) -> bool {
    let sender = sender.unwrap_or_else(|| ctx.checker.get_type_at_location(sender_node));
    state.types.mismatch(ctx.checker, sender, receiver) && !safe_bitwise(ctx, sender_node, receiver)
}

fn check_assignment(
    state: &mut State,
    ctx: &mut RuleContext<'_>,
    receiver: TypeId,
    sender: Node,
    reporting: Node,
    place: Unsafe,
    sender_type: Option<TypeId>,
) {
    let sender = skip_parentheses(sender);
    // The literal listeners check each element and property.
    if is_array_literal_expression(sender)
        || is_object_literal_expression(sender)
        || !unsafe_assignment(state, ctx, sender, receiver, sender_type)
    {
        return;
    }
    report(ctx, reporting, place, &[receiver]);
    mark_checked(&mut state.checked, sender);
}

fn check_arguments(
    state: &mut State,
    ctx: &mut RuleContext<'_>,
    node: Node,
    arguments: impl Iterator<Item = Node>,
    tagged: bool,
) {
    let Some(mut signature) = FunctionSignature::new(ctx.checker, node) else {
        return;
    };
    if tagged {
        // The first parameter of a tag takes the template strings.
        signature.next();
    }
    for argument in arguments {
        if is_spread_element(argument) {
            let spread_type = ctx.checker.get_type_at_location(argument.expression());
            if ctx.checker.is_tuple_type(spread_type) {
                let mut mismatches = Vec::new();
                for element in type_arguments(ctx.checker, spread_type) {
                    if let Some(parameter) = signature.next()
                        && state.types.mismatch(ctx.checker, element, parameter)
                    {
                        mismatches.push(parameter);
                    }
                }
                if !mismatches.is_empty() {
                    report(ctx, argument, Unsafe::Argument, &mismatches);
                }
                let combined_flags = ctx.checker.target_tuple_type(spread_type).combined_flags;
                if combined_flags.intersects(ElementFlags::VARIABLE) {
                    signature.consumed = true;
                }
                continue;
            }
        }
        if let Some(parameter) = signature.next() {
            check_assignment(
                state,
                ctx,
                parameter,
                argument,
                argument,
                Unsafe::Argument,
                None,
            );
        }
    }
}

fn check_return(state: &mut State, ctx: &mut RuleContext<'_>, sender: Node, reporting: Node) {
    let function = get_parent_function_node(sender);
    if function.is_nil() {
        return;
    }
    let signature = ctx
        .checker
        .get_signature_from_declaration_exported(function);
    if signature.is_nil() {
        return;
    }
    let mut receiver = ctx.checker.get_return_type_of_signature(signature);
    let mut sender_type = ctx.checker.get_type_at_location(sender);
    if is_async_function(function) {
        receiver = ctx.checker.get_awaited_type(receiver);
        sender_type = ctx.checker.get_awaited_type(sender_type);
    }
    if receiver.is_some() && sender_type.is_some() {
        check_assignment(
            state,
            ctx,
            receiver,
            sender,
            reporting,
            Unsafe::Return,
            Some(sender_type),
        );
    }
}

fn check_mutation(ctx: &mut RuleContext<'_>, target: Node, reporting: Node) {
    let target_type = ctx.checker.get_type_at_location(target);
    let constrained = constraint(ctx.checker, target_type);
    if !get_enum_types(ctx.checker, constrained).is_empty() {
        report(ctx, reporting, Unsafe::Mutation, &[target_type]);
    }
}

fn check_initializer(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    let initializer = node.initializer();
    if initializer.is_some() {
        let receiver = ctx.checker.get_type_at_location(node.name());
        check_assignment(
            state,
            ctx,
            receiver,
            initializer,
            node,
            Unsafe::Assignment,
            None,
        );
    }
}

fn check_assertion(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    let receiver = ctx.checker.get_type_at_location(node.type_());
    check_assignment(
        state,
        ctx,
        receiver,
        node.expression(),
        node,
        Unsafe::Assertion,
        None,
    );
}

fn check_property_declaration(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    let initializer = node.initializer();
    if initializer.is_nil() {
        return;
    }
    let name = node.name();
    let member_name = if is_computed_property_name(name) {
        let name_type = ctx.checker.get_type_at_location(name.expression());
        if is_type_flag_set(
            ctx.checker,
            name_type,
            TypeFlags::STRING_LITERAL | TypeFlags::NUMBER_LITERAL,
        ) {
            literal_type_value(ctx.checker, name_type).map(ToString::to_string)
        } else {
            None
        }
    } else if is_identifier(name) || is_string_literal(name) || is_numeric_literal(name) {
        Some(name.text().to_owned())
    } else {
        None
    };

    // The types that the base classes and interfaces give the member.
    let mut heritage_types = Vec::new();
    if let Some(member_name) = &member_name {
        let is_static = is_static(node);
        for clause in get_heritage_clauses(node.parent()).nodes() {
            if is_static && clause.token() != SyntaxKind::ExtendsKeyword {
                continue;
            }
            for heritage in clause.types().nodes() {
                let heritage_node = if is_static {
                    heritage.expression()
                } else {
                    heritage
                };
                let heritage_type = ctx.checker.get_type_at_location(heritage_node);
                let symbol = ctx.checker.get_property_of_type(heritage_type, member_name);
                if symbol.is_some() {
                    heritage_types.push(ctx.checker.get_type_of_symbol(symbol));
                }
            }
        }
    }
    if !heritage_types.is_empty()
        && heritage_types
            .iter()
            .all(|&heritage_type| unsafe_assignment(state, ctx, initializer, heritage_type, None))
    {
        report(ctx, node, Unsafe::Assignment, &heritage_types);
        mark_checked(&mut state.checked, initializer);
        return;
    }
    let receiver = ctx.checker.get_type_at_location(node);
    check_assignment(
        state,
        ctx,
        receiver,
        initializer,
        node,
        Unsafe::Assignment,
        None,
    );
}

fn check_binary_expression(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    if !is_assignment_expression(node, false) {
        return;
    }
    match node.operator_token().kind() {
        SyntaxKind::EqualsToken
        | SyntaxKind::AmpersandAmpersandEqualsToken
        | SyntaxKind::AmpersandEqualsToken
        | SyntaxKind::QuestionQuestionEqualsToken
        | SyntaxKind::CaretEqualsToken
        | SyntaxKind::BarEqualsToken
        | SyntaxKind::BarBarEqualsToken => {
            let receiver = ctx.checker.get_type_at_location(node.left());
            check_assignment(
                state,
                ctx,
                receiver,
                node.right(),
                node,
                Unsafe::Assignment,
                None,
            );
        }
        _ => check_mutation(ctx, node.left(), node),
    }
}

fn check_jsx_attribute(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    let initializer = node.initializer();
    if initializer.is_nil() || !is_jsx_expression(initializer) {
        return;
    }
    let expression = initializer.expression();
    if expression.is_nil() {
        return;
    }
    let receiver = ctx
        .checker
        .get_contextual_type(expression, ContextFlags::NONE);
    if receiver.is_some() {
        check_assignment(
            state,
            ctx,
            receiver,
            expression,
            expression,
            Unsafe::Assignment,
            None,
        );
    }
}

fn check_array_literal(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    if state.checked.contains(&node) {
        return;
    }
    for element in node.elements() {
        if element.kind() == SyntaxKind::OmittedExpression {
            continue;
        }
        let receiver = ctx.checker.get_contextual_type(element, ContextFlags::NONE);
        if receiver.is_some() {
            check_assignment(
                state,
                ctx,
                receiver,
                element,
                element,
                Unsafe::Assignment,
                None,
            );
        }
    }
}

fn check_object_literal(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    if state.checked.contains(&node) {
        return;
    }
    for property in node.properties() {
        let (sender, receiver_node) = match property.kind() {
            SyntaxKind::SpreadAssignment => (property.expression(), node),
            SyntaxKind::PropertyAssignment => (property.initializer(), property.initializer()),
            SyntaxKind::ShorthandPropertyAssignment => (property.name(), property.name()),
            _ => continue,
        };
        let receiver = ctx
            .checker
            .get_contextual_type(receiver_node, ContextFlags::NONE);
        if receiver.is_some() {
            check_assignment(
                state,
                ctx,
                receiver,
                sender,
                property,
                Unsafe::Assignment,
                None,
            );
        }
    }
}

fn check_element_access(state: &mut State, ctx: &mut RuleContext<'_>, node: Node) {
    let symbol = ctx
        .checker
        .get_symbol_at_location_exported(node.expression());
    if symbol.is_nil() {
        return;
    }
    let declarations = ctx.checker.sym(symbol).declarations.clone();
    let mut receivers = Vec::new();
    for declaration in declarations {
        receivers.extend(mapped_key_constraint_types(ctx.checker, declaration));
    }
    if receivers.is_empty() {
        return;
    }
    let argument = node.argument_expression();
    let sender = ctx.checker.get_type_at_location(argument);
    if receivers.iter().all(|&receiver| {
        state.types.mismatch(ctx.checker, sender, receiver)
            || !ctx.checker.is_type_assignable_to(sender, receiver)
    }) {
        report(ctx, argument, Unsafe::Access, &receivers);
    }
}

// The parameters of a call.

/// Walks the parameter types of the resolved signature as arguments consume
/// them.
struct FunctionSignature {
    /// The types of the parameters before the rest parameter.
    parameters: Vec<TypeId>,
    /// The element types of the rest parameter. Empty when there is none.
    rest: Vec<TypeId>,
    rest_index: usize,
    index: usize,
    /// A spread of unknown length came before: only the last rest type is
    /// known to take the next argument.
    consumed: bool,
}

impl FunctionSignature {
    fn new(c: &mut Checker, node: Node) -> Option<Self> {
        let signature = c.get_resolved_signature(node, None, CheckMode::NORMAL);
        if signature.is_nil() {
            return None;
        }
        let mut result = FunctionSignature {
            parameters: Vec::new(),
            rest: Vec::new(),
            rest_index: 0,
            index: 0,
            consumed: false,
        };
        let parameters = c.sig(signature).parameters().to_vec();
        for (i, parameter) in parameters.into_iter().enumerate() {
            let parameter_type = c.get_type_of_symbol_at_location(parameter, node);
            let constrained = constraint(c, parameter_type);
            let is_rest = c
                .sym(parameter)
                .declarations
                .first()
                .is_some_and(|&declaration| is_rest_parameter_declaration(declaration));
            if is_rest {
                result.rest_index = i;
                if c.is_tuple_type(constrained) {
                    result.rest = type_arguments(c, constrained).to_vec();
                } else {
                    let element = get_number_index_type(c, constrained);
                    result.rest = vec![if element.is_some() {
                        element
                    } else {
                        constrained
                    }];
                }
                break;
            }
            result.parameters.push(parameter_type);
        }
        Some(result)
    }

    /// The type of the parameter that takes the next argument.
    fn next(&mut self) -> Option<TypeId> {
        let index = self.index;
        self.index += 1;
        if index < self.parameters.len() && !self.consumed {
            return Some(self.parameters[index]);
        }
        let last = *self.rest.last()?;
        if self.consumed {
            return Some(last);
        }
        Some(
            self.rest
                .get(index - self.rest_index)
                .copied()
                .unwrap_or(last),
        )
    }
}

// The comparison of two types.

const MAXIMUM_TYPE_COMPARISON_DEPTH: usize = 5;

/// A sender and a receiver, and whether they are members of a generic type.
type ComparisonVisit = (TypeId, TypeId, bool);

/// The answers that one file keeps about its types.
#[derive(Default)]
struct EnumTypes {
    generic_types: FxHashMap<TypeId, bool>,
    mismatches: FxHashMap<(TypeId, TypeId), bool>,
}

/// The base constraint of `t`, or `t` when it has none.
fn constraint(c: &mut Checker, t: TypeId) -> TypeId {
    let constraint = c.get_base_constraint_of_type(t);
    if constraint.is_some() { constraint } else { t }
}

/// The type arguments of a type reference. Empty for other types.
fn type_arguments(c: &mut Checker, t: TypeId) -> SharedList<TypeId> {
    if is_object_type(c, t) && c.ty(t).object_flags.intersects(ObjectFlags::REFERENCE) {
        c.get_type_arguments(t)
    } else {
        SharedList::from(&[][..])
    }
}

impl EnumTypes {
    /// Whether `t` has a type parameter in it or in its type arguments.
    fn is_generic(&mut self, c: &mut Checker, t: TypeId) -> bool {
        if let Some(&generic) = self.generic_types.get(&t) {
            return generic;
        }
        // A type that contains itself asks for its own answer.
        self.generic_types.insert(t, false);
        let mut generic = is_type_flag_set(c, t, TypeFlags::INSTANTIABLE);
        if !generic && is_type_flag_set(c, t, TypeFlags::UNION_OR_INTERSECTION) {
            generic = c
                .ty(t)
                .types_list()
                .into_iter()
                .any(|part| self.is_generic(c, part));
        }
        if !generic && let Some(alias) = c.ty(t).alias.clone() {
            generic = alias
                .type_arguments
                .iter()
                .any(|&argument| self.is_generic(c, argument));
        }
        if !generic {
            generic = type_arguments(c, t)
                .into_iter()
                .any(|argument| self.is_generic(c, argument));
        }
        self.generic_types.insert(t, generic);
        generic
    }

    /// Whether `sender`, or a type inside it, does not fit the enums that
    /// `receiver` has at the same place.
    fn mismatch(&mut self, c: &mut Checker, sender: TypeId, receiver: TypeId) -> bool {
        if let Some(&mismatch) = self.mismatches.get(&(sender, receiver)) {
            return mismatch;
        }
        let mismatch = self.deep_mismatch(c, sender, receiver, &mut FxHashMap::default(), 0, false);
        self.mismatches.insert((sender, receiver), mismatch);
        mismatch
    }

    fn deep_mismatch(
        &mut self,
        c: &mut Checker,
        sender: TypeId,
        receiver: TypeId,
        visited: &mut FxHashMap<ComparisonVisit, usize>,
        depth: usize,
        within_generic_members: bool,
    ) -> bool {
        if depth > MAXIMUM_TYPE_COMPARISON_DEPTH {
            return false;
        }
        let sender = constraint(c, sender);
        let receiver = constraint(c, receiver);
        if sender == receiver {
            return false;
        }
        let visit = (sender, receiver, within_generic_members);
        if visited
            .get(&visit)
            .is_some_and(|&previous_depth| previous_depth <= depth)
        {
            return false;
        }
        visited.insert(visit, depth);
        if direct_mismatch(c, sender, receiver) {
            return true;
        }
        let sender_arguments = type_arguments(c, sender);
        let receiver_arguments = type_arguments(c, receiver);
        if sender_arguments.len() == receiver_arguments.len() {
            for (argument, receiver_argument) in
                sender_arguments.into_iter().zip(receiver_arguments)
            {
                if self.deep_mismatch(
                    c,
                    argument,
                    receiver_argument,
                    visited,
                    depth + 1,
                    within_generic_members,
                ) {
                    return true;
                }
            }
        }
        let generic = self.is_generic(c, sender) || self.is_generic(c, receiver);
        if generic && within_generic_members {
            return false;
        }
        let within_generic_members = generic || within_generic_members;
        let sender_element = get_number_index_type(c, sender);
        let receiver_element = get_number_index_type(c, receiver);
        if sender_element.is_some()
            && receiver_element.is_some()
            && self.deep_mismatch(
                c,
                sender_element,
                receiver_element,
                visited,
                depth + 1,
                within_generic_members,
            )
        {
            return true;
        }
        for receiver_property in c.get_properties_of_type(receiver) {
            let name = c.sym(receiver_property).name.clone();
            let sender_property = c.get_property_of_type(sender, &name);
            if sender_property.is_nil() {
                continue;
            }
            let sender_type = c.get_type_of_symbol(sender_property);
            let receiver_type = c.get_type_of_symbol(receiver_property);
            if self.deep_mismatch(
                c,
                sender_type,
                receiver_type,
                visited,
                depth + 1,
                within_generic_members,
            ) {
                return true;
            }
        }
        false
    }
}

/// Every union part has an intersection part with one of `flags`.
fn primitive_like(c: &Checker, t: TypeId, flags: TypeFlags) -> bool {
    union_type_parts(c, t).iter().all(|&union_part| {
        intersection_type_parts(c, union_part)
            .iter()
            .any(|&part| is_type_flag_set(c, part, flags))
    })
}

/// Whether a part of `sender` is a number or a string that `receiver` takes
/// only as a member of an enum that the part is not of.
fn direct_mismatch(c: &mut Checker, sender: TypeId, receiver: TypeId) -> bool {
    let receiver_enums = get_enum_types(c, receiver);
    let mut non_enums = Vec::new();
    let (mut number_enum, mut string_enum) = (false, false);
    for part in union_type_parts(c, receiver) {
        if is_type_flag_set(c, part, TypeFlags::ENUM_LIKE) {
            if is_type_flag_set(c, part, TypeFlags::NUMBER_LITERAL) {
                number_enum = true;
            } else {
                string_enum = true;
            }
        }
        if get_enum_types(c, part).is_empty() {
            non_enums.push(part);
        }
    }
    for part in union_type_parts(c, sender) {
        if !(number_enum && primitive_like(c, part, TypeFlags::NUMBER_LIKE)
            || string_enum && primitive_like(c, part, TypeFlags::STRING_LIKE))
        {
            continue;
        }
        let shared = get_enum_types(c, part)
            .iter()
            .any(|sender_enum| receiver_enums.contains(sender_enum));
        if !shared
            && !non_enums
                .iter()
                .any(|&non_enum| c.is_type_assignable_to(part, non_enum))
        {
            return true;
        }
    }
    false
}

/// The quoted names of the enums in `types`, sorted.
fn describe(c: &mut Checker, types: &[TypeId]) -> String {
    let mut names = BTreeSet::new();
    let mut visited = FxHashMap::default();
    for &t in types {
        describe_type(c, t, 0, &mut names, &mut visited);
    }
    names.into_iter().collect::<Vec<_>>().join(", ")
}

fn describe_type(
    c: &mut Checker,
    t: TypeId,
    depth: usize,
    names: &mut BTreeSet<String>,
    visited: &mut FxHashMap<TypeId, usize>,
) {
    if depth > MAXIMUM_TYPE_COMPARISON_DEPTH {
        return;
    }
    let t = constraint(c, t);
    if visited
        .get(&t)
        .is_some_and(|&previous_depth| previous_depth <= depth)
    {
        return;
    }
    visited.insert(t, depth);
    for enum_type in get_enum_types(c, t) {
        names.insert(format!("'{}'", c.type_to_string(enum_type)));
    }
    for argument in type_arguments(c, t) {
        describe_type(c, argument, depth + 1, names, visited);
    }
    let element = get_number_index_type(c, t);
    if element.is_some() {
        describe_type(c, element, depth + 1, names, visited);
    }
    for property in c.get_properties_of_type(t) {
        let property_type = c.get_type_of_symbol(property);
        describe_type(c, property_type, depth + 1, names, visited);
    }
}

/// The key types that the declared type of `declaration` maps over, when
/// they have enums.
fn mapped_key_constraint_types(c: &mut Checker, declaration: Node) -> Vec<TypeId> {
    if !matches!(
        declaration.kind(),
        SyntaxKind::GetAccessor
            | SyntaxKind::Parameter
            | SyntaxKind::PropertyDeclaration
            | SyntaxKind::PropertySignature
            | SyntaxKind::VariableDeclaration
    ) {
        return Vec::new();
    }
    let type_node = declaration.type_();
    if type_node.is_nil() {
        return Vec::new();
    }
    if is_type_reference_node(type_node) {
        let t = c.get_type_from_type_node(type_node);
        if is_object_type(c, t) && c.ty(t).object_flags.intersects(ObjectFlags::MAPPED) {
            let symbol = c.ty(t).symbol;
            if symbol.is_some()
                && c.sym(symbol).declarations.iter().any(|&declaration| {
                    is_mapped_type_node(declaration) && declaration.name_type().is_some()
                })
            {
                return Vec::new();
            }
            // Resolving the members sets the constraint type.
            c.get_properties_of_type(t);
            let constraint_type = c.ty(t).as_mapped_type().constraint_type();
            if constraint_type.is_some() {
                let constrained = constraint(c, constraint_type);
                if !get_enum_types(c, constrained).is_empty() {
                    return vec![constraint_type];
                }
            }
        }
    }
    if is_mapped_type_node(type_node) {
        let constraint_node = type_node.type_parameter().constraint();
        if constraint_node.is_some() {
            return vec![c.get_type_from_type_node(constraint_node)];
        }
    }
    let mut result = Vec::new();
    if is_type_literal_node(type_node) {
        for member in type_node.members() {
            let name = member.name();
            if name.is_some() && is_computed_property_name(name) {
                result.push(c.get_type_at_location(name.expression()));
            }
        }
    }
    result
}
