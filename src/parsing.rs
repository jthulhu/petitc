use std::{collections::HashMap, path::Path};

use crate::ast::{
    BinOp, DeclOrInstr, Expr, File, FunDecl, Ident, Instr, Type, VarDecl,
};

use crate::error::Result;
use beans::error::WarningSet;
use beans::include_parser;
use beans::parser::{Parser, Value, AST};
use beans::stream::StringStream;

macro_rules! error {
    ($($tok:tt)*) => {{
	eprintln!(
	    "File \"{}\", line {}, character {}:\nInternal error:",
	    std::file!(),
	    std::line!(),
	    std::column!(),
	);
	eprintln!($($tok)*);
	::std::process::exit(2);
    }};
}

macro_rules! get {
    ($node:expr, $key:literal) => {
        $node.remove($key).unwrap_or_else(|| {
            error!("expected to find child {}, got\n{:?}", $key, $node)
        })
    };
}

macro_rules! node {
    ($node:expr) => {
        if let AST::Node { attributes, .. } = $node {
            attributes
        } else {
            error!("expected to find node");
        }
    };
}

macro_rules! value {
    ($node:expr, $key:literal) => {
        if let AST::Literal(Value::Str(result)) = get!($node, $key) {
            result
        } else {
            error!("expected to find value, got\n{:?}", $node);
        }
    };
}

macro_rules! match_variant {
    (($node:expr) {
	$($variant:literal => $code:expr),* $(,)?
    }) => {
	let variant = value!($node, "variant");
	match &*variant {
	    $($variant => $code,)*
	    found_variant => error!("Unexpected variant {}", found_variant),
	}
    };
}

fn read_ident(
    ident: String,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> usize {
    *string_assoc.entry(ident.clone()).or_insert_with(|| {
        string_store.push(ident);
        string_store.len()
    })
}

fn read_nonempty_list<T>(
    f: impl Fn(AST, bool, &mut Vec<String>, &mut HashMap<String, usize>) -> T,
    mut ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Vec<T> {
    let mut result = Vec::new();
    loop {
        let mut node = node!(ast);
        match_variant! {(node) {
            "Cons" => {
            result.push(f(get!(node, "head"), toplevel, string_store, string_assoc));
            ast = get!(node, "tail");
            },
            "Nil" => {
            result.push(f(get!(node, "head"), toplevel, string_store, string_assoc));
            break;
            },
        }}
    }
    result
}

fn read_option<T>(
    f: impl Fn(AST, bool, &mut Vec<String>, &mut HashMap<String, usize>) -> T,
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Option<T> {
    let mut node = node!(ast);
    match_variant! {(node) {
    "None" => None,
    "Some" => Some(f(get!(node, "value"), toplevel, string_store, string_assoc)),
    }}
}

fn read_list<T>(
    f: impl Fn(AST, bool, &mut Vec<String>, &mut HashMap<String, usize>) -> T,
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Vec<T> {
    let mut node = node!(ast);
    read_option(
        |ast, toplevel, string_store, string_assoc| {
            read_nonempty_list(&f, ast, toplevel, string_store, string_assoc)
        },
        get!(node, "value"),
        toplevel,
        string_store,
        string_assoc,
    )
    .unwrap_or_default()
}

fn read_else(
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Instr {
    let mut node = node!(ast);
    read_statement(get!(node, "else"), toplevel, string_store, string_assoc)
}

fn read_statement(
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Instr {
    let mut node = node!(ast);
    match_variant! {(node) {
    "None" => Instr::EmptyInstr,
    "Regular" => Instr::ExprInstr(read_expr(get!(node, "stmt"), toplevel, string_store, string_assoc)),
    "If" => Instr::If {
        cond: read_expr(get!(node, "condition"), toplevel, string_store, string_assoc),
        then_branch: Box::new(read_statement(get!(node, "then"), toplevel, string_store, string_assoc)),
        else_branch: read_option(
            |ast, toplevel, string_store, string_assoc| {
                Box::new(read_else(ast, toplevel, string_store, string_assoc))
            },
            get!(node, "else"),
            toplevel,
            string_store,
            string_assoc
        ),
    },
    "While" => Instr::While {
        cond: read_expr(get!(node, "condition"), toplevel, string_store, string_assoc),
        body: Box::new(read_statement(get!(node, "body"), toplevel, string_store, string_assoc)),
    },
    "For" => Instr::For {
        loop_var: read_option(read_variable_declaration, get!(node, "init"), toplevel, string_store, string_assoc),
        cond: read_option(read_expr, get!(node, "test"), toplevel, string_store, string_assoc),
        incr: read_list(read_expr, get!(node, "step"), toplevel, string_store, string_assoc),
        body: Box::new(read_statement(get!(node, "body"), toplevel, string_store, string_assoc)),
    },
    "Block" => Instr::Block(read_block(get!(node, "stmts"), toplevel, string_store, string_assoc)),
    "Return" => Instr::Return(read_option(read_expr, get!(node, "value"), toplevel, string_store, string_assoc)),
    "Break" => Instr::Break,
    "Continue" => Instr::Continue,
    }}
}

fn read_int(ast: AST) -> Expr {
    let mut node = node!(ast);
    match_variant! {(node) {
    "Int" => Expr::Int(value!(node, "value").parse().unwrap()),
    "Char" => Expr::Int(value!(node, "value").as_bytes()[0].into()),
    }}
}

fn read_bool(ast: AST) -> Expr {
    let mut node = node!(ast);
    match_variant! {(node) {
    "True" => Expr::True,
    "False" => Expr::False,
    }}
}

fn read_op(ast: AST) -> BinOp {
    use BinOp::*;
    let mut node = node!(ast);
    match_variant! {(node) {
    "Mul" => Mul,
    "Div" => Div,
    "Mod" => Mod,
    "Add" => Add,
    "Sub" => Sub,
    "Lt" => Lt,
    "Gt" => Gt,
    "Geq" => Ge,
    "Leq" => Le,
    "Equal" => Eq,
    "NotEqual" => NEq,
    "And" => BAnd,
    "Or" => BOr,
    }}
}

fn read_expr(
    ast: AST,
    _toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, Ident>,
) -> Expr {
    let mut node = node!(ast);
    match_variant! {(node) {
    "Int" => read_int(get!(node, "value")),
    "Bool" => read_bool(get!(node, "value")),
    "Null" => Expr::Null,
    "Through" => read_expr(get!(node, "this"), _toplevel, string_store, string_assoc),
    "Not" => Expr::Not(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Incrl" => Expr::PrefixIncr(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Incrr" => Expr::PostfixIncr(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Decrl" => Expr::PrefixDecr(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Decrr" => Expr::PostfixDecr(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Borrow" => Expr::Addr(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Deref" => Expr::Deref(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Plus" => Expr::Pos(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Minus" => Expr::Neg(Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
    ))),
    "Ident" => Expr::Ident(read_ident(
        value!(node, "value").to_string(),
        string_store,
        string_assoc,
    )),
    "Sizeof" => Expr::SizeOf(read_type(get!(node, "type"))),
    "Call" => Expr::Call {
        name: read_ident(
        value!(node, "name").to_string(),
        string_store,
        string_assoc,
        ),
        args: read_list(
        read_expr,
        get!(node, "args"),
        _toplevel,
        string_store,
        string_assoc,
        ),
    },
    "BinOp" => Expr::Op {
        op: read_op(get!(node, "op")),
        lhs: Box::new(read_expr(
        get!(node, "left"),
        _toplevel,
        string_store,
        string_assoc,
        )),
        rhs: Box::new(read_expr(
        get!(node, "right"),
        _toplevel,
        string_store,
        string_assoc,
        )),
    },
    "Assign" => Expr::Assign {
        lhs: Box::new(read_expr(
        get!(node, "key"),
        _toplevel,
        string_store,
        string_assoc,
        )),
        rhs: Box::new(read_expr(
        get!(node, "value"),
        _toplevel,
        string_store,
        string_assoc,
        )),
    },
    }}
}

fn read_definition(
    ast: AST,
    _toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Expr {
    let mut node = node!(ast);
    read_expr(get!(node, "value"), _toplevel, string_store, string_assoc)
}

fn read_variable_declaration(
    ast: AST,
    _toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> VarDecl {
    let mut node = node!(ast);
    VarDecl {
        ty: read_type(get!(node, "type")),
        name: read_ident(
            value!(node, "name").to_string(),
            string_store,
            string_assoc,
        ),
        value: read_option(
            read_definition,
            get!(node, "value"),
            _toplevel,
            string_store,
            string_assoc,
        ),
    }
}

fn read_declaration_or_statement(
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> DeclOrInstr {
    let mut node = node!(ast);
    match_variant! {(node) {
    "Function" => DeclOrInstr::Fun(read_fun_decl(
        get!(node, "declaration"), false, string_store, string_assoc
    )),
    "Declaration" => DeclOrInstr::Var(read_variable_declaration(
        get!(node, "declaration"),
        toplevel,
        string_store,
        string_assoc,
    )),
    "Statement" => DeclOrInstr::Instr(read_statement(
        get!(node, "stmt"), toplevel, string_store, string_assoc
    )),
    }}
}

fn read_block(
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> Vec<DeclOrInstr> {
    let mut node = node!(ast);
    read_list(
        read_declaration_or_statement,
        get!(node, "stmts"),
        toplevel,
        string_store,
        string_assoc,
    )
}

fn read_type(ast: AST) -> Type {
    let mut node = node!(ast);
    match_variant! {(node) {
        "Void" => Type::VOID,
        "Int" => Type::INT,
        "Bool" => Type::BOOL,
        "Pointer" => read_type(get!(node, "pointed")).ptr()
    }}
}

fn read_typed_param(
    ast: AST,
    _toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> (Type, usize) {
    let mut node = node!(ast);
    (
        read_type(get!(node, "type")),
        read_ident(
            value!(node, "name").to_string(),
            string_store,
            string_assoc,
        ),
    )
}

fn read_fun_decl(
    ast: AST,
    toplevel: bool,
    string_store: &mut Vec<String>,
    string_assoc: &mut HashMap<String, usize>,
) -> FunDecl {
    let mut node = node!(ast);
    FunDecl {
        ty: read_type(get!(node, "rettype")),
        name: read_ident(
            value!(node, "name").to_string(),
            string_store,
            string_assoc,
        ),
        params: read_list(
            read_typed_param,
            get!(node, "args"),
            toplevel,
            string_store,
            string_assoc,
        ),
        code: read_block(
            get!(node, "block"),
            toplevel,
            string_store,
            string_assoc,
        ),
        toplevel,
    }
}

fn read_file(ast: AST) -> (File, Vec<String>) {
    let mut node = node!(ast);
    let mut string_store = vec!["malloc".into(), "putchar".into()];
    let mut string_assoc = HashMap::new();
    string_assoc.insert("malloc".into(), 0);
    string_assoc.insert("putchar".into(), 1);
    (
        File {
            fun_decls: read_list(
                read_fun_decl,
                get!(node, "decls"),
                true,
                &mut string_store,
                &mut string_assoc,
            ),
        },
        Vec::new(),
    )
}

/// Return a file and a string vector v such that
/// v[id] represent the ident id
/// v[0] is guaranteed to be malloc
/// and v[1] to be putchar
pub(crate) fn parse_to_ast(source: &Path) -> Result<(File, Vec<String>)> {
    let mut warnings = WarningSet::empty();
    let (lexer, parser) = include_parser!(
        lexer => compiled "../gmrs/petitc.clx",
        parser => compiled "../gmrs/petitc.cgr",
    )?
    .unpack_into(&mut warnings);
    let mut input = StringStream::from_file(source)?.unpack_into(&mut warnings);
    let ast = parser
        .parse(&mut lexer.lex(&mut input))?
        .unpack_into(&mut warnings);
    Ok(read_file(ast.tree))
}
