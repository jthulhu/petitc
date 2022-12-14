use crate::{cwrite, cwriteln, typechecker::PartialType, typing::Type};
use beans::{error::Error as BeansError, span::Span};
use std::{fmt, io::Error as IoError};
use thiserror::Error;

macro_rules! error {
    ($f:expr, $($rest:tt)*) => {{
	let string = $crate::color::cformat!($($rest)*);
	cwriteln!($f, "<s,r!>error</><s,w!>: {}</>", string)
    }};
}

const NB_LINES_SHOWN: usize = 3;

#[derive(Debug, Error)]
pub struct Error {
    kind: Box<ErrorKind>,
    reason: Option<String>,
    helps: Vec<String>,
}

impl Error {
    pub(crate) fn reason(self, reason: String) -> Self {
        Error {
            reason: Some(reason),
            ..self
        }
    }

    pub(crate) fn add_help(mut self, help: String) -> Self {
        self.helps.push(help);
        self
    }

    pub(crate) fn new(kind: ErrorKind) -> Self {
        Error {
            kind: Box::new(kind),
            reason: None,
            helps: Vec::new(),
        }
    }
}

impl<T> From<T> for Error
where
    ErrorKind: From<T>,
{
    fn from(error: T) -> Self {
        Self::new(ErrorKind::from(error))
    }
}

impl fmt::Display for Error {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        let mut helps = self.helps.clone();
        match &*self.kind {
            ErrorKind::Beans(BeansError::SyntaxError {
                name,
                alternatives,
                location,
            }) => {
                let span = location.get();
                display::span(span, f)?;
                error!(f, "the token {} cannot be recognized here", name)?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
                let alternative_message = match &alternatives[..] {
                    [] => String::from(
                        "no token would make sense here, what have you done?",
                    ),
                    [alternative] => {
                        format!("try inserting {alternative} right before")
                    }
                    [starts @ .., last] => {
                        format!(
                            "{} or {last} were expected here",
                            starts.join(", ")
                        )
                    }
                };
                helps.push(alternative_message);
            }
            ErrorKind::Beans(BeansError::SyntaxErrorValidPrefix {
                location,
            }) => {
                let span = location.get();
                display::span(span, f)?;
                error!(f, "this file is syntactically valid but incomplete")?;
                display::pretty_span(
                    span,
                    "^",
                    "the rest of the program is missing",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::Beans(BeansError::LexingError { location }) => {
                let span = location.get();
                display::span(span, f)?;
                error!(f, "this file cannot be lexed")?;
                display::pretty_span(
                    span,
                    "^",
                    "unable to recognize a token here",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::Beans(BeansError::UnwantedToken { span, message }) => {
                let span = span.get();
                display::span(span, f)?;
                error!(f, "{}", message)?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
            }
            ErrorKind::Beans(other) => {
                writeln!(
                    f,
                    "The parsing engine encountered an internal error:{}",
                    other
                )?;
            }
            ErrorKind::IoError(e) => {
                writeln!(f, "The engine encountered a io error:{}", e)?
            }
            ErrorKind::NoMainFunction => {
                error!(f, "expected to find symbol `main` at toplevel")?;
            }
            ErrorKind::IncorrectMainFunctionType { ty, params, span } => {
                display::span(span, f)?;
                error!(
		    f,
		    "signature mismatch, `main` should be of type `int()`, not `{}({})`",
		    ty,
		    params.iter().map(ToString::to_string).collect::<Vec<_>>().join(", "),
		)?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
            }
            ErrorKind::BreakContinueOutsideLoop { span } => {
                display::span(span, f)?;
                error!(f, "break or continue statement outside of a loop")?;
                display::pretty_span(
                    span,
                    "^",
                    "`break` and `continue` must be within loops",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::NameError { name, span } => {
                display::span(span, f)?;
                error!(f, "symbol `{}` is undefined.", name)?;
                display::pretty_span(
                    span,
                    "^",
                    "not found in this scope",
                    "-->",
                    true,
                    f,
                )?;
            }
	    ErrorKind::SymbolIsFunction { name, span, definition_span } => {
		display::span(span, f)?;
		error!(f, "symbol `{}` is a function, not a variable", name)?;
		let message = format!("`{}` is used as a variable", name);
		if let Some(definition_span) = definition_span {
		    display::pretty_span_hint(
			span,
			message,
			definition_span,
			format!("`{}` is defined here", name),
			f,
		    )?;
		} else {
		    display::pretty_span(span, "^", message, "-->", true, f)?;
		}
		helps.push(format!("maybe you meant to call `{name}`"));
	    }
	    ErrorKind::SymbolIsVariable {
		name,
		span,
		definition_span,
	    } => {
		display::span(span, f)?;
		error!(f, "symbol `{}` is a varaible, not a function", name)?;
		let message = format!("`{}` is used as a function", name);
		if let Some(definition_span) = definition_span {
		    display::pretty_span_hint(
			span,
			message,
			definition_span,
			format!("`{}` is defined here", name),
			f,
		    )?;
		} else {
		    display::pretty_span(span, "^", message, "-->", true, f)?;
		}
	    }
            ErrorKind::AddressOfRvalue {
                span,
                expression_span,
            } => {
                display::span(span, f)?;
                error!(f, "cannot take the address of an rvalue expression")?;
                display::pretty_span(
                    expression_span,
                    "^",
                    "this expression has no address in memory",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::DerefNonPointer { ty, span } => {
                display::span(span, f)?;
                error!(f, "cannot cast `{}` as `_*`", ty)?;
                display::pretty_span(
                    span,
                    "^",
                    "this is not a pointer, it cannot be dereferenced",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::SymbolDefinedTwice {
                first_definition,
                second_definition,
                name,
            } => {
                display::span(second_definition, f)?;
                error!(
                    f,
                    "the symbol `{}` is defined twice in the same block", name,
                )?;
                display::pretty_span_hint(
                    second_definition,
                    "second definition found here",
                    first_definition,
                    "first definition found here",
                    f,
                )?;
            }
            ErrorKind::ArityMismatch {
                found_arity,
                expected_arity,
                span,
                definition_span,
                function_name,
            } => {
                display::span(span, f)?;
                error!(
		    f,
		    "arity mismatch between this function call and its definition.",
		)?;

                if let Some(definition_span) = definition_span {
                    display::pretty_span_hint(
                        span,
                        format!("found {} arguments", found_arity),
                        definition_span,
                        format!("expected {} arguments", expected_arity),
                        f,
                    )?;
                } else {
                    display::pretty_span(
                        span,
                        "^",
                        format!("found {} arguments", found_arity),
                        "-->",
                        true,
                        f,
                    )?;
                    let nb_line_length =
                        (span.end().0 + 1).to_string().len().max(3);
                    cwriteln!(
                        f,
                        "{} <s,b!>=</> <s,w!>note:</> `{}` is a builtin function which takes {} arguments",
                        " ".repeat(nb_line_length),
			function_name,
			expected_arity,
                    )?;
                }
            }
            ErrorKind::FunctionDefinedTwice {
                first_definition,
                second_definition,
                name,
            } => {
                display::span(second_definition, f)?;
                error!(
                    f,
                    "the symbol `{}` is defined twice at the toplevel", name
                )?;
                if let Some(first_definition) = first_definition {
                    display::pretty_span_hint(
                        second_definition,
                        "second definition found here",
                        first_definition,
                        "first definition found here",
                        f,
                    )?;
                } else {
                    display::pretty_span(
                        second_definition,
                        "^",
                        "definition found here",
                        "-->",
                        true,
                        f,
                    )?;
                    let nb_line_length = (second_definition.end().0 + 1)
                        .to_string()
                        .len()
                        .max(3);
                    cwriteln!(
                        f,
                        "{} <s,b!>=</> <s,w!>note:</> `{}` is a builtin function",
                        " ".repeat(nb_line_length),
			name,
                    )?;
                }
            }
            ErrorKind::VoidVariable { span, name } => {
                display::span(span, f)?;
                error!(
                    f,
                    "this symbol `{}` has been declared with type `void`", name,
                )?;
                display::pretty_span(
                    span,
                    "^",
                    "`void` is not a valid type for a variable",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::VariableTypeMismatch {
                span,
                definition_span,
                expected_type,
                found_type,
                variable_name,
            } => {
                display::span(span, f)?;
                error!(
		    f,
		    "the symbol `{}` has been defined with type `{}`, but it was assigned `{}`",
		    variable_name,
		    expected_type,
		    found_type,
		)?;
                display::pretty_span_hint(
                    span,
                    "",
                    definition_span,
                    format!("`{}` is defined here", variable_name),
                    f,
                )?;
            }
            ErrorKind::DerefVoidPointer { span } => {
                display::span(span, f)?;
                error!(f, "`void*` cannot be dereferenced",)?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
                cwriteln!(
		    f,
		    "  <s,b!>=</> <s,w!>help:</> try casting this value to an other pointer type before dereferencing it",
		)?;
            }
            ErrorKind::VoidExpression { span } => {
                display::span(span, f)?;
                error!(
                    f,
                    "this expression has type `void`, which is forbidden",
                )?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
                cwriteln!(
		    f,
		    "  <s,b!>=</> <s,w!>note:</> expressions with type `void` can only be discarded"
		)?;
            }
            ErrorKind::SizeofVoid { span } => {
                display::span(span, f)?;
                error!(f, "`void` does not have a size",)?;
                display::pretty_span(span, "^", "", "-->", true, f)?;
                cwriteln!(f, "  <s,b!>=</> <s,w!>hint: <i>use gcc!</></>")?;
            }
            ErrorKind::RvalueAssignment { span } => {
                display::span(span, f)?;
                error!(
		    f,
		    "only expressions that are <i>lvalues</> can be assigned to",
		)?;
                display::pretty_span(
                    span,
                    "^",
                    "cannot assign to this expression",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::TypeMismatch {
                span,
                expected_type,
                found_type,
                origin_span,
            } => {
                display::span(span, f)?;
                error!(
                    f,
                    "expected type `{}`, found `{}` instead",
                    expected_type,
                    found_type,
                )?;
                if let Some(origin_span) = origin_span {
                    display::pretty_span_hint(
                        span,
                        "this expression has a wrong type",
                        origin_span,
                        "conflicting type declaration",
                        f,
                    )?;
                } else {
                    display::pretty_span(
                        span,
                        "^",
                        "this expression has a wrong type",
                        "-->",
                        true,
                        f,
                    )?;
                }
            }
            ErrorKind::IncrOrDecrRvalue {
                span,
                expression_span,
            } => {
                display::span(span, f)?;
                error!(f, "only <i>lvalue</> expressions can be mutated")?;
                display::pretty_span(
                    expression_span,
                    "^",
                    "cannot mutate this expression",
                    "-->",
                    true,
                    f,
                )?;
            }
            ErrorKind::BuiltinBinopTypeMismatch {
                left_type,
                right_type,
                span,
                op,
            } => {
                display::span(span, f)?;
                error!(
                    f,
                    "invalid operands to binary `{}` for `{}` and `{}`",
                    op,
                    left_type,
                    right_type,
                )?;
                display::pretty_span(
                    span,
                    "^",
                    "invalid arithmetic operation",
                    "-->",
                    true,
                    f,
                )?;
            }
        };
        for help in helps {
            cwriteln!(f, "  <s,b!>=</> <s,w!>help:</> {}", help)?;
        }
        if let Some(ref reason) = self.reason {
            cwriteln!(
		f,
		"  <s,b!>=</> <s,w!>note:</> this error was triggered because {}",
		reason
	    )?;
        }
        Ok(())
    }
}

#[derive(Debug)]
pub enum ErrorKind {
    Beans(BeansError),
    IoError(IoError),
    AddressOfRvalue {
        span: Span,
        expression_span: Span,
    },
    DerefNonPointer {
        ty: Type,
        span: Span,
    },
    NoMainFunction,
    IncorrectMainFunctionType {
        ty: Type,
        params: Vec<PartialType>,
        span: Span,
    },
    BreakContinueOutsideLoop {
        span: Span,
    },
    NameError {
        name: String,
        span: Span,
    },
    SymbolDefinedTwice {
        first_definition: Span,
        second_definition: Span,
        name: String,
    },
    FunctionDefinedTwice {
        first_definition: Option<Span>,
        second_definition: Span,
        name: String,
    },
    ArityMismatch {
        found_arity: usize,
        expected_arity: usize,
        span: Span,
        definition_span: Option<Span>,
        function_name: String,
    },
    VoidVariable {
        span: Span,
        name: String,
    },
    VoidExpression {
        span: Span,
    },
    DerefVoidPointer {
        span: Span,
    },
    VariableTypeMismatch {
        span: Span,
        definition_span: Span,
        expected_type: PartialType,
        found_type: PartialType,
        variable_name: String,
    },
    SizeofVoid {
        span: Span,
    },
    RvalueAssignment {
        span: Span,
    },
    TypeMismatch {
        span: Span,
        expected_type: PartialType,
        found_type: PartialType,
	origin_span: Option<Span>,
    },
    IncrOrDecrRvalue {
        span: Span,
        expression_span: Span,
    },
    SymbolIsFunction {
	name: String,
	span: Span,
	definition_span: Option<Span>,
    },
    SymbolIsVariable {
	name: String,
	span: Span,
	definition_span: Option<Span>,
    },
    BuiltinBinopTypeMismatch {
        left_type: PartialType,
        right_type: PartialType,
        span: Span,
        op: &'static str,
    },
}

impl From<BeansError> for ErrorKind {
    fn from(error: BeansError) -> Self {
        Self::Beans(error)
    }
}

impl From<IoError> for ErrorKind {
    fn from(error: IoError) -> Self {
        Self::IoError(error)
    }
}

pub(crate) mod display {
    use super::*;

    pub fn span(span: &Span, f: &mut fmt::Formatter) -> fmt::Result {
        write!(
            f,
            "File \"{}\", ",
            span.file().file_name().unwrap().to_str().unwrap()
        )?;
        locations(span.start(), span.end(), f)?;
        writeln!(f, ":")
    }

    pub fn locations(
        (start_line, start_column): (usize, usize),
        (end_line, end_column): (usize, usize),
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        if start_line == end_line {
            write!(f, "line {}, ", start_line + 1)?;
            if start_column == end_column {
                write!(f, "character {}", start_column)
            } else {
                write!(f, "characters {}-{}", start_column, end_column)
            }
        } else {
            write!(
                f,
                "lines {}-{}, characters {}-{}",
                start_line + 1,
                end_line + 1,
                start_column,
                end_column
            )
        }
    }

    pub fn single_line(
        span: &Span,
        left_column_size: usize,
        padding: impl fmt::Display,
        line_nb: usize,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        let (line_begins_at, line_ends_at) = span.line_bytes_of_line(line_nb);
        left_column_with(left_column_size, line_nb + 1, f)?;
        cwriteln!(
            f,
            "<s,r!>{}</>{}",
            padding,
            span.text()[line_begins_at..line_ends_at].trim_end(),
        )
    }

    pub fn left_column_with(
        left_column_size: usize,
        content: impl fmt::Display,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        cwrite!(
            f,
            "{: <left_column_size$} <s,b!>|</> ",
            content,
            left_column_size = left_column_size,
        )
    }

    pub fn left_column(
        left_column_size: usize,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        cwrite!(f, "{} <s,b!>|</> ", " ".repeat(left_column_size),)
    }

    pub fn left_column_newline(
        left_column_size: usize,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        left_column(left_column_size, f)?;
        writeln!(f)
    }

    pub fn error_report(
        span: &Span,
        left_column_size: usize,
        arrow: impl fmt::Display,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        cwriteln!(
            f,
            "{}<s,b!>{}</> {}:{}:{}",
            " ".repeat(left_column_size),
            arrow,
            span.file().file_name().unwrap().to_str().unwrap(),
            span.start().0 + 1,
            span.start().1,
        )
    }

    pub fn comment(
        underline_filler: &'static str,
        offset: usize,
        padding: impl fmt::Display,
        offset_char: &'static str,
        length: usize,
        left_column_size: usize,
        message: impl fmt::Display,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        left_column(left_column_size, f)?;
        cwriteln!(
            f,
            "<s,r!>{}{}</><s,r!>{} {}</>",
            padding,
            offset_char.repeat(offset),
            underline_filler.repeat(length),
            message,
        )
    }

    pub fn pretty_span_hint(
        span: &Span,
        message: impl fmt::Display,
        hint_span: &Span,
        hint_message: impl fmt::Display,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
	let left_column_size = (span.start().0+1)
	    .to_string()
	    .len()
	    .max((span.start().0 + 1).to_string().len())
	    .max(3);
        if hint_span.start().0 == hint_span.end().0
            && span.start().0 == span.end().0
            && span.start().0 == hint_span.end().0
        {
            error_report(span, left_column_size, "-->", f)?;
            left_column_newline(left_column_size, f)?;
            single_line(span, left_column_size, "", span.start().0, f)?;
            let is_span_first = hint_span.start().1 > span.end().1;
            left_column(left_column_size, f)?;
            if is_span_first {
                let offset1 = span.start().1;
                let length1 = span.end().1 - span.start().1 + 1;
                let offset2 = hint_span.start().1 - (offset1 + length1);
                let length2 = hint_span.end().1 - hint_span.start().1 + 1;
                cwriteln!(
                    f,
                    "{}<s,r!>{}</>{}<s,r!>{} {}</>",
                    " ".repeat(offset1),
                    "^".repeat(length1),
                    " ".repeat(offset2),
                    "~".repeat(length2),
                    hint_message,
                )?;
                left_column(left_column_size, f)?;
                cwriteln!(
                    f,
                    "{}<s,r!>{}</>",
                    " ".repeat(offset1 + length1 - 1),
                    message,
                )?;
            } else {
                let offset1 = hint_span.start().1;
                let length1 = hint_span.end().1 - hint_span.start().1 + 1;
                let offset2 = span.start().1 - (offset1 + length1);
                let length2 = span.end().1 - span.start().1 + 1;
                cwriteln!(
                    f,
                    "{}<s,r!>{}</>{}<s,r!>{} {}</>",
                    " ".repeat(offset1),
                    "~".repeat(length1),
                    " ".repeat(offset2),
                    "^".repeat(length2),
                    message,
                )?;
                left_column(left_column_size, f)?;
                cwriteln!(
                    f,
                    "{}<s,r!>{}</>",
                    " ".repeat(offset1 + length1 - 1),
                    hint_message,
                )?;
            }
        } else if hint_span.end() <= span.start() {
            pretty_span(hint_span, "~", hint_message, "-->", true, f)?;
	    if span.file() == hint_span.file()
		&& hint_span.end().0 + 1 < span.start().0
	    {
		left_column_with(left_column_size, "...", f)?;
		writeln!(f)?;
	    };
            pretty_span(
                span,
                "^",
                message,
                ":::",
                span.file() != hint_span.file(),
                f,
            )?;
        } else {
            pretty_span(span, "^", message, "-->", true, f)?;
	    if span.file() == hint_span.file()
		&& hint_span.end().0 + 1 < span.start().0
	    {
		left_column_with(left_column_size, "...", f)?;
		writeln!(f)?;
	    };
            pretty_span(
                hint_span,
                "~",
                hint_message,
                ":::",
                span.file() != hint_span.file(),
                f,
            )?;
        }
        Ok(())
    }

    pub fn pretty_span(
        span: &Span,
        underline: &'static str,
        message: impl fmt::Display,
        arrow: impl fmt::Display,
        show_line: bool,
        f: &mut fmt::Formatter,
    ) -> fmt::Result {
        let left_column_size = (span.start().0 + 1).to_string().len().max(3);
        if show_line {
            error_report(span, left_column_size, arrow, f)?;
            left_column_newline(left_column_size, f)?;
        };
        if span.start().0 != span.end().0 {
            single_line(span, left_column_size, "  ", span.start().0, f)?;
            comment(
                underline,
                span.start_byte() - span.lines()[span.start().0],
                " _",
                "_",
                1,
                left_column_size,
                "",
                f,
            )?;

            if span.end().0 - span.start().0 <= 2 * NB_LINES_SHOWN {
                for line in span.start().0 + 1..span.end().0 {
                    single_line(span, left_column_size, "| ", line, f)?;
                }
            } else {
                for line in span.start().0 + 1..span.start().0 + NB_LINES_SHOWN
                {
                    single_line(span, left_column_size, "| ", line, f)?;
                }
                left_column_with(left_column_size, "...", f)?;
                cwriteln!(f, "<s,r!>|</> ")?;
                for line in span.end().0 - NB_LINES_SHOWN..span.end().0 {
                    single_line(span, left_column_size, "| ", line, f)?;
                }
            }
            single_line(span, left_column_size, "| ", span.end().0, f)?;
            comment(
                underline,
                span.end_byte() - span.lines()[span.end().0],
                " -",
                "-",
                1,
                left_column_size,
                message,
                f,
            )?;
        } else {
            single_line(span, left_column_size, "", span.start().0, f)?;
            comment(
                underline,
                span.start_byte() - span.lines()[span.start().0],
                "",
                " ",
                span.end().1 - span.start().1 + 1,
                left_column_size,
                message,
                f,
            )?;
        }
        Ok(())
    }
}

pub type Result<T> = std::result::Result<T, Error>;
