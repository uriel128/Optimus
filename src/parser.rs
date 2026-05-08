use crate::ast::{
    AssignmentTarget, BinaryOperator, ClassField, Expression, FunctionDecl, Literal, Parameter,
    Statement, UnaryOperator,
};
use crate::lexer::Token;
use chumsky::prelude::*;

#[derive(Debug, Clone)]
enum Postfix {
    Call(Vec<Expression>),
    Member(String),
    Index(Expression), // NEW – arr[i], map["key"]
}

#[derive(Debug, Clone)]
enum ClassItem {
    Field(ClassField),
    Method(FunctionDecl),
}

fn build_member_expression(base: String, members: &[String]) -> Expression {
    let mut expr = Expression::Identifier(base);
    for member in members {
        expr = Expression::MemberAccess {
            object: Box::new(expr),
            member: member.clone(),
        };
    }
    expr
}

fn build_assignment_target(base: String, members: Vec<String>) -> AssignmentTarget {
    if members.is_empty() {
        AssignmentTarget::Identifier(base)
    } else {
        let last_index = members.len() - 1;
        let object = build_member_expression(base, &members[..last_index]);
        AssignmentTarget::MemberAccess {
            object,
            member: members[last_index].clone(),
        }
    }
}

pub fn parser() -> impl Parser<Token, Vec<Statement>, Error = Simple<Token>> {
    let ident = select! { Token::Identifier(name) => name };

    let type_name = select! {
        Token::IntType    => "int".to_string(),
        Token::FloatType  => "float".to_string(),
        Token::StringType => "string".to_string(),
        Token::BoolType   => "bool".to_string(),
        Token::Identifier(name) => name,
    };

    let expr = recursive(|expr| {
        let literal = select! {
            Token::Integer(n) => Expression::Literal(Literal::Int(n)),
            Token::Float(s)   => Expression::Literal(Literal::Float(s.parse().unwrap_or(0.0))),
            Token::String(s)  => Expression::Literal(Literal::Str(s)),
            Token::True       => Expression::Literal(Literal::Bool(true)),
            Token::False      => Expression::Literal(Literal::Bool(false)),
            Token::Null       => Expression::Literal(Literal::Null),
            Token::None       => Expression::Literal(Literal::Null), // NEW – none is alias for null
        };

        let call_args = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let new_expr = just(Token::New)
            .ignore_then(ident.clone())
            .then(call_args.clone())
            .map(|(class_name, arguments)| Expression::New { class_name, arguments });

        let array_literal = expr
            .clone()
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LBracket), just(Token::RBracket))
            .map(Expression::Array);

        let atom = choice((
            new_expr,
            array_literal, // NEW
            literal,
            ident.clone().map(Expression::Identifier),
            expr.clone()
                .delimited_by(just(Token::LParen), just(Token::RParen)),
        ));

        let postfix = call_args
            .map(Postfix::Call)
            .or(just(Token::Dot)
                .ignore_then(ident.clone())
                .map(Postfix::Member))
            .or(expr
                .clone()
                .delimited_by(just(Token::LBracket), just(Token::RBracket))
                .map(Postfix::Index));

        let postfixed = atom
            .clone()
            .then(postfix.repeated())
            .foldl(|left, postfix| match postfix {
                Postfix::Call(arguments) => Expression::Call {
                    callee: Box::new(left),
                    arguments,
                },
                Postfix::Member(member) => Expression::MemberAccess {
                    object: Box::new(left),
                    member,
                },
           
                Postfix::Index(index) => Expression::Index {
                    collection: Box::new(left),
                    index: Box::new(index),
                },
            });

        let unary = just(Token::Minus)
            .to(UnaryOperator::Negate)
            .or(just(Token::Bang).to(UnaryOperator::Not))
            .repeated()
            .then(postfixed)
            .foldr(|operator, rhs| Expression::UnaryOp {
                operator,
                expr: Box::new(rhs),
            });

        let product = unary
            .clone()
            .then(
                just(Token::Asterisk)
                    .to(BinaryOperator::Multiply)
                    .or(just(Token::Slash).to(BinaryOperator::Divide))
                    .then(unary)
                    .repeated(),
            )
            .foldl(|left, (operator, right)| Expression::BinaryOp {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            });

        let sum = product
            .clone()
            .then(
                just(Token::Plus)
                    .to(BinaryOperator::Add)
                    .or(just(Token::Minus).to(BinaryOperator::Subtract))
                    .then(product)
                    .repeated(),
            )
            .foldl(|left, (operator, right)| Expression::BinaryOp {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            });

        let comparison = sum
            .clone()
            .then(
                just(Token::Less)
                    .to(BinaryOperator::Less)
                    .or(just(Token::Greater).to(BinaryOperator::Greater))
                    .then(sum)
                    .repeated(),
            )
            .foldl(|left, (operator, right)| Expression::BinaryOp {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            });

        comparison
            .clone()
            .then(
                just(Token::Equals)
                    .to(BinaryOperator::Equal)
                    .or(just(Token::NotEquals).to(BinaryOperator::NotEqual))
                    .then(comparison)
                    .repeated(),
            )
            .foldl(|left, (operator, right)| Expression::BinaryOp {
                left: Box::new(left),
                operator,
                right: Box::new(right),
            })
    });

    let stmt = recursive(|stmt| {
        let block = stmt
            .clone()
            .repeated()
            .delimited_by(just(Token::LBrace), just(Token::RBrace))
            .map(Statement::Block);


        let var_decl = just(Token::Mut)
            .or_not()
            .then(type_name.clone())
            .then(ident.clone())
            .then_ignore(just(Token::Assign))
            .then(expr.clone())
            .map(|(((opt_mut, var_type), name), value)| Statement::VariableDecl {
                is_mutable: opt_mut.is_some(),
                var_type,
                name,
                value,
            });

        let let_decl = just(Token::Mut)
            .or_not()
            .then_ignore(just(Token::Let))
            .then(ident.clone())
            .then_ignore(just(Token::Assign))
            .then(expr.clone())
            .map(|((opt_mut, name), value)| Statement::LetDecl {
                is_mutable: opt_mut.is_some(),
                name,
                value,
            });


        let assignment_no_semicolon = ident
            .clone()
            .then(just(Token::Dot).ignore_then(ident.clone()).repeated())
            .then_ignore(just(Token::Assign))
            .then(expr.clone())
            .map(|((base, members), value)| Statement::Assignment {
                target: build_assignment_target(base, members),
                value,
            });

        let index_assign_no_semicolon = ident
            .clone()
            .then(
                expr.clone()
                    .delimited_by(just(Token::LBracket), just(Token::RBracket)),
            )
            .then_ignore(just(Token::Assign))
            .then(expr.clone())
            .map(|((name, index), value)| Statement::Assignment {
                target: AssignmentTarget::Index {
                    collection: Expression::Identifier(name),
                    index,
                },
                value,
            });

        let var_decl_stmt       = var_decl.clone().then_ignore(just(Token::Semicolon));
        let let_decl_stmt       = let_decl.then_ignore(just(Token::Semicolon));
        let assignment_stmt     = assignment_no_semicolon.clone().then_ignore(just(Token::Semicolon));
        let index_assign_stmt   = index_assign_no_semicolon.clone().then_ignore(just(Token::Semicolon));


        let print_stmt = just(Token::Print)
            .ignore_then(
                expr.clone()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then_ignore(just(Token::Semicolon))
            .map(Statement::Print);

        let return_stmt = just(Token::Return)
            .ignore_then(expr.clone().or_not())
            .then_ignore(just(Token::Semicolon))
            .map(Statement::Return);

        let break_stmt = just(Token::Break)
            .then_ignore(just(Token::Semicolon))
            .map(|_| Statement::Break);


        let parameter = type_name
            .clone()
            .then(ident.clone())
            .map(|(type_name, name)| Parameter { type_name, name });

        let params = parameter
            .separated_by(just(Token::Comma))
            .allow_trailing()
            .delimited_by(just(Token::LParen), just(Token::RParen));

        let function_decl_data = just(Token::Fn)
            .ignore_then(ident.clone())
            .then(params.clone())
            .then(just(Token::Colon).ignore_then(type_name.clone()).or_not())
            .then(block.clone())
            .map(|(((name, params), return_type), body)| FunctionDecl {
                name,
                params,
                return_type,
                body: Box::new(body),
            });

        let function_decl_stmt = function_decl_data.clone().map(Statement::FunctionDecl);


        let class_field = just(Token::Mut)
            .or_not()
            .then(type_name.clone())
            .then(ident.clone())
            .then(just(Token::Assign).ignore_then(expr.clone()).or_not())
            .then_ignore(just(Token::Semicolon))
            .map(|(((opt_mut, field_type), name), default_value)| ClassField {
                is_mutable: opt_mut.is_some(),
                field_type,
                name,
                default_value,
            });

        let class_item = class_field
            .map(ClassItem::Field)
            .or(function_decl_data.clone().map(ClassItem::Method));

        let class_decl = just(Token::Class)
            .ignore_then(ident.clone())
            .then(
                class_item
                    .repeated()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(|(name, items)| {
                let mut fields = Vec::new();
                let mut methods = Vec::new();
                for item in items {
                    match item {
                        ClassItem::Field(f)  => fields.push(f),
                        ClassItem::Method(m) => methods.push(m),
                    }
                }
                Statement::ClassDecl { name, fields, methods }
            });


        let module_decl = just(Token::Module)
            .ignore_then(ident.clone())
            .then(
                stmt.clone()
                    .repeated()
                    .delimited_by(just(Token::LBrace), just(Token::RBrace)),
            )
            .map(|(name, body)| Statement::ModuleDecl { name, body });

        let import_stmt = just(Token::Import)
            .ignore_then(ident.clone())
            .then_ignore(just(Token::Semicolon))
            .map(Statement::Import);


        let if_stmt = recursive(|if_stmt| {
            just(Token::If)
                .ignore_then(
                    expr.clone()
                        .delimited_by(just(Token::LParen), just(Token::RParen)),
                )
                .then(block.clone())
                .then(
                    just(Token::Else)
                        .ignore_then(block.clone().or(if_stmt.clone()))
                        .or_not(),
                )
                .map(|((condition, then_branch), else_branch)| Statement::If {
                    condition,
                    then_branch: Box::new(then_branch),
                    else_branch: else_branch.map(Box::new),
                })
        });

        let while_loop = just(Token::While)
            .ignore_then(
                expr.clone()
                    .delimited_by(just(Token::LParen), just(Token::RParen)),
            )
            .then(block.clone())
            .map(|(condition, body)| Statement::WhileLoop {
                condition,
                body: Box::new(body),
            });

        

        let for_init = var_decl
            .clone()
            .or(assignment_no_semicolon.clone())
            .or(expr.clone().map(Statement::Expression));

        let for_increment = assignment_no_semicolon
            .clone()
            .or(expr.clone().map(Statement::Expression));

        let for_stmt = just(Token::For).ignore_then(
            just(Token::LParen)
                .ignore_then(for_init)
                .then_ignore(just(Token::Semicolon))
                .then(expr.clone())
                .then_ignore(just(Token::Semicolon))
                .then(for_increment)
                .then_ignore(just(Token::RParen))
                .then(block.clone())
                .map(|(((init, condition), increment), body)| Statement::ForLoop {
                    init: Box::new(init),
                    condition,
                    increment: Box::new(increment),
                    body: Box::new(body),
                })
                .or(
                    ident
                        .clone()
                        .then_ignore(just(Token::In))
                        .then(expr.clone())
                        .then(block.clone())
                        .map(|((var, iterable), body)| Statement::ForIn {
                            var,
                            iterable,
                            body: Box::new(body),
                        }),
                ),
        );

        let expr_stmt = expr
            .clone()
            .then_ignore(just(Token::Semicolon))
            .map(Statement::Expression);

        function_decl_stmt
            .or(class_decl)
            .or(module_decl)
            .or(import_stmt)
            .or(let_decl_stmt)       
            .or(var_decl_stmt)
            .or(index_assign_stmt)   
            .or(assignment_stmt)
            .or(print_stmt)
            .or(return_stmt)
            .or(break_stmt)          
            .or(for_stmt)            
            .or(if_stmt)
            .or(while_loop)
            .or(block)
            .or(expr_stmt)
    });

    stmt.repeated().then_ignore(end())
}