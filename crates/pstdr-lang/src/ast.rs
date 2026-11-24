use crate::diagnostics::Span;

#[derive(Debug, Clone, PartialEq)]
pub struct Program {
    pub header: Option<AddonDirective>,
    pub bind_targets: Vec<BindTargetStmt>,
    pub items: Vec<TopLevelItem>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct AddonDirective {
    pub target: String,
    pub api: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct BindTargetStmt {
    pub path: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum TopLevelItem {
    ConstDecl(VarDecl),
    MutDecl(VarDecl),
    StateDecl(StateDecl),
    FuncDecl(FuncDecl),
    UnsafeFuncDecl(UnsafeFuncDecl),
    UnsafeVarDecl(UnsafeVarDecl),
    EventHandler(EventHandler),
    AfterTimer {
        duration: DurationLit,
        body: Block,
        span: Span,
    },
    EveryTimer {
        duration: DurationLit,
        body: Block,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct VarDecl {
    pub name: String,
    pub name_span: Span,
    pub value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct StateDecl {
    pub name: String,
    pub owner_hint: String,
    pub default_value: Expr,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct FuncDecl {
    pub name: String,
    pub params: Vec<(String, Span)>,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnsafeFuncDecl {
    pub name: String,
    pub params: Vec<String>,
    pub raw_body: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct UnsafeVarDecl {
    pub keyword: String,
    pub name: String,
    pub raw_rest: String,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EventHandler {
    pub kind: EventKind,
    pub body: Block,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum EventKind {
    RightClick {
        item: Expr,
        actor_var: String,
    },
    HitTarget {
        target_var: String,
        item: Expr,
        actor_var: String,
    },
    BreakBlock {
        block_id: Expr,
        actor_var: String,
    },
    PlayerJoin {
        actor_var: String,
    },
    Hurt {
        target_var: String,
        attacker_var: Option<String>,
    },
    Die {
        target_var: String,
        killer_var: Option<String>,
    },
    Consume {
        item: Expr,
        actor_var: String,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub struct Block {
    pub stmts: Vec<Stmt>,
    pub span: Span,
}

#[derive(Debug, Clone, PartialEq)]
pub enum Stmt {
    ConstDecl(VarDecl),
    MutDecl(VarDecl),
    Assign {
        target: AssignTarget,
        op: AssignOp,
        value: Expr,
        span: Span,
    },
    If {
        condition: Expr,
        then_block: Block,
        else_branch: Option<ElseBranch>,
        span: Span,
    },
    For {
        var: String,
        var_span: Span,
        iter: Expr,
        body: Block,
        span: Span,
    },
    While {
        condition: Expr,
        body: Block,
        span: Span,
    },
    Return {
        value: Option<Expr>,
        span: Span,
    },
    CancelEvent {
        span: Span,
    },
    Damage {
        target: Expr,
        amount: Expr,
        span: Span,
    },
    Ignite {
        target: Expr,
        duration: DurationLit,
        span: Span,
    },
    ApplyEffect {
        effect: String,
        effect_span: Span,
        level: Expr,
        target: Expr,
        duration: DurationLit,
        span: Span,
    },
    SendMessage {
        message: Expr,
        target: Expr,
        span: Span,
    },
    SpawnParticle {
        particle: Expr,
        location: Expr,
        dimension: Option<Expr>,
        span: Span,
    },
    After {
        duration: DurationLit,
        body: Block,
        span: Span,
    },
    Every {
        duration: DurationLit,
        body: Block,
        span: Span,
    },
    ExprStmt {
        expr: Expr,
        span: Span,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ElseBranch {
    ElseIf(Box<Stmt>),
    Else(Block),
}

#[derive(Debug, Clone, PartialEq)]
pub enum AssignTarget {
    Ident(String, Span),
    Index {
        object: Expr,
        index: Expr,
        span: Span,
    },
    Field {
        object: Expr,
        field: String,
        span: Span,
    },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AssignOp {
    Assign,
    AddAssign,
    SubAssign,
    MulAssign,
    DivAssign,
}

impl AssignOp {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Assign => "=",
            Self::AddAssign => "+=",
            Self::SubAssign => "-=",
            Self::MulAssign => "*=",
            Self::DivAssign => "/=",
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct DurationLit {
    pub raw: String,
    pub ms: f64,
    pub ticks: f64,
    pub span: Span,
}

impl DurationLit {
    pub fn ticks_rounded(&self) -> u64 {
        self.ticks.round().max(0.0) as u64
    }

    pub fn seconds_f64(&self) -> f64 {
        self.ms / 1000.0
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum StringPart {
    Literal(String),
    Expr(Expr),
}

#[derive(Debug, Clone, PartialEq)]
pub struct ObjectField {
    pub key: String,
    pub key_is_string: bool,
    pub value: Expr,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnaryOp {
    Not,
    Neg,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinaryOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    Eq,
    Neq,
    Lt,
    Le,
    Gt,
    Ge,
    And,
    Or,
    Nullish,
    In,
    Within,
}

#[derive(Debug, Clone, PartialEq)]
pub enum IsPredicate {
    Sneaking,
    Sprinting,
    Swimming,
    Flying,
    Sleeping,
    OnGround,
    InWater,
    Holding(Box<Expr>),
}

#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    Number(f64, Span),
    Bool(bool, Span),
    Null(Span),
    StringParts(Vec<StringPart>, Span),
    Duration(DurationLit),
    Distance(f64, Span),
    Ident(String, Span),
    Object(Vec<ObjectField>, Span),
    Array(Vec<Expr>, Span),
    Index {
        object: Box<Expr>,
        index: Box<Expr>,
        span: Span,
    },
    Member {
        object: Box<Expr>,
        field: String,
        optional: bool,
        span: Span,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
        span: Span,
    },
    Unary {
        op: UnaryOp,
        expr: Box<Expr>,
        span: Span,
    },
    Binary {
        left: Box<Expr>,
        op: BinaryOp,
        right: Box<Expr>,
        span: Span,
    },
    IsCheck {
        subject: Box<Expr>,
        negated: bool,
        predicate: IsPredicate,
        span: Span,
    },
    Ternary {
        cond: Box<Expr>,
        then_expr: Box<Expr>,
        else_expr: Box<Expr>,
        span: Span,
    },
    Paren(Box<Expr>, Span),
}

impl Expr {
    pub fn span(&self) -> &Span {
        match self {
            Self::Number(_, s)
            | Self::Bool(_, s)
            | Self::Null(s)
            | Self::StringParts(_, s)
            | Self::Distance(_, s)
            | Self::Ident(_, s)
            | Self::Object(_, s)
            | Self::Array(_, s)
            | Self::Index { span: s, .. }
            | Self::Member { span: s, .. }
            | Self::Call { span: s, .. }
            | Self::Unary { span: s, .. }
            | Self::Binary { span: s, .. }
            | Self::IsCheck { span: s, .. }
            | Self::Ternary { span: s, .. }
            | Self::Paren(_, s) => s,
            Self::Duration(d) => &d.span,
        }
    }
}
