//! Regular expression pattern checks.

use std::cell::Cell;
use std::rc::Rc;

use goport_core::flags::ScriptTarget;
use goport_core::frontend::scanner::{RegularExpressionFlags, new_reg_exp_parser, new_scanner};

/// The diagnostics for an octal escape, for a backreference to a group that
/// does not exist, and for both in a character class.
const ANNEX_B_ESCAPE_CODES: [u32; 5] = [1487, 1533, 1534, 1536, 1537];

/// Whether `pattern` compiles as a regular expression without flags.
///
/// tsgolint compiles with `regexp2.Compile(pattern, regexp2.ECMAScript)`.
/// This runs TypeScript's own regular expression checker in its Annex B
/// (no `u` or `v` flag) mode instead, which accepts what `new
/// RegExp(pattern)` accepts. The two agree on the patterns that rules see
/// in practice (unbalanced brackets and groups, quantifiers without an
/// atom, ...); .NET-only syntax such as `(?i)` is accepted by regexp2 only.
///
/// TypeScript also reports a decimal escape that is not a backreference
/// (`\1` without a group, `\08`, `[\8]`) as a likely mistake. Annex B reads it
/// as a legacy octal escape or an identity escape, and so does regexp2, so
/// those diagnostics do not make the pattern invalid.
pub fn is_valid_regexp_pattern(pattern: &str) -> bool {
    let invalid = Rc::new(Cell::new(false));
    let mut scanner = new_scanner();
    scanner.set_script_target(ScriptTarget::LATEST);
    scanner.set_text(pattern);
    let on_error = invalid.clone();
    scanner.set_on_error(Some(Box::new(move |message, _, _, _| {
        if !ANNEX_B_ESCAPE_CODES.contains(&message.code()) {
            on_error.set(true);
        }
    })));
    scanner.reset_pos(0);
    let end = i32::try_from(pattern.len()).expect("pattern length fits in i32");
    let named_capture_groups = has_named_capture_group(pattern);
    let mut parser = new_reg_exp_parser(
        &mut scanner,
        end,
        RegularExpressionFlags::NONE,
        false,
        false,
        true,
        named_capture_groups,
    );
    parser.run();
    !invalid.get()
}

/// A `(?<name>` group outside a character class, found the way the scanner
/// finds it when it rescans a regular expression literal.
fn has_named_capture_group(pattern: &str) -> bool {
    let bytes = pattern.as_bytes();
    let (mut in_escape, mut in_class) = (false, false);
    for (i, &ch) in bytes.iter().enumerate() {
        if in_escape {
            in_escape = false;
        } else if ch == b'\\' {
            in_escape = true;
        } else if ch == b'[' {
            in_class = true;
        } else if ch == b']' {
            in_class = false;
        } else if !in_class
            && bytes[i..].starts_with(b"(?<")
            && !matches!(bytes.get(i + 3), Some(b'=' | b'!'))
        {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::is_valid_regexp_pattern;

    #[test]
    fn valid_and_invalid_patterns() {
        assert!(is_valid_regexp_pattern("^[a-z]+thing/?$"));
        assert!(is_valid_regexp_pattern("bar"));
        assert!(is_valid_regexp_pattern("(?<year>\\d{4})-\\k<year>"));
        assert!(is_valid_regexp_pattern("a\\1\\8\\377\\08(b)\\2[\\1\\8]"));
        assert!(!is_valid_regexp_pattern("[a-z"));
        assert!(!is_valid_regexp_pattern("(a"));
        assert!(!is_valid_regexp_pattern("*a"));
        assert!(!is_valid_regexp_pattern("a\\"));
    }
}
