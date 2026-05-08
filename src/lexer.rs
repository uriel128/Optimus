use logos::Logos;

#[derive(Logos, Debug, Clone, PartialEq, Eq, Hash)]
#[logos(skip r"[ \t\r\n\f]+")]
#[logos(skip r"//[^\n]*")]
pub enum Token {
    #[token("fn")]     Fn,
    #[token("class")]  Class,
    #[token("module")] Module,
    #[token("import")] Import,
    #[token("if")]     If,
    #[token("else")]   Else,
    #[token("while")]  While,
    #[token("for")]    For,
    #[token("in")]     In,     
    #[token("break")]  Break,   
    #[token("return")] Return,
    #[token("print")]  Print,
    #[token("new")]    New,
    #[token("mut")]    Mut,
    #[token("let")]    Let,     

    #[token("int")]    IntType,
    #[token("float")]  FloatType,
    #[token("string")] StringType,
    #[token("bool")]   BoolType,

    
    #[token("true")]  True,
    #[token("false")] False,
    #[token("null")]  Null,
    #[token("none")]  None,   // NEW – spec alias for null

    #[regex(r"\d+\.\d*|\.\d+", |lex| lex.slice().to_string())]
    Float(String),

    #[regex(r"\d+", |lex| lex.slice().parse::<i64>().unwrap_or(0))]
    Integer(i64),

    #[regex(r#""[^"]*""#, |lex| {
        let s = lex.slice();
        s[1..s.len() - 1].to_string()
    })]
    String(String),

    #[regex(r"[a-zA-Z_][a-zA-Z0-9_]*", |lex| lex.slice().to_string())]
    Identifier(String),

    #[token("==")] Equals,
    #[token("!=")] NotEquals,

    #[token("+")] Plus,
    #[token("-")] Minus,
    #[token("*")] Asterisk,
    #[token("/")] Slash,
    #[token("!")] Bang,
    #[token("<")] Less,
    #[token(">")] Greater,
    #[token("=")] Assign,

    #[token("(")] LParen,
    #[token(")")] RParen,
    #[token("{")] LBrace,
    #[token("}")] RBrace,
    #[token("[")] LBracket,  
    #[token("]")] RBracket,  

    #[token(";")] Semicolon,
    #[token(",")] Comma,
    #[token(".")] Dot,
    #[token(":")] Colon,
}