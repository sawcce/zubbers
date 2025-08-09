use std::{
    cell::RefCell,
    fmt::{format, write, Debug},
    ops::{Add, Deref, Div, Mul, Rem, Sub},
    rc::Rc,
};

use super::{
    BinaryOp, Binding, Expr, ExprNode, IrBuilder, IrFunction, IrFunctionBody, Literal, Type,
    TypeInfo,
};

use log::{info, trace, warn};

pub trait Generate: Debug {
    fn generate(&self, context: &mut IrBuilder) -> ExprNode;
    fn type_info(&self, context: &IrBuilder) -> TypeInfo;
    fn emit(&self, context: &mut IrBuilder) {
        let gen = self.generate(context);
        context.program.push(gen);
    }

    fn boxed(self) -> Box<Self>
    where
        Self: Generate + Sized,
    {
        Box::new(self)
    }

    fn ret(self) -> Return<Self>
    where
        Self: Generate + Sized,
    {
        Return { value: self }
    }

    fn call<'v>(self, args: Vec<Box<dyn Generate>>) -> Call<Self>
    where
        Self: Generate + Sized,
    {
        Call { callee: self, args }
    }

    fn if_true_do<'a, T, F>(self, body: T) -> Conditional<'a, Self, T, F>
    where
        Self: Generate + Sized,
    {
        Conditional {
            condition: self,
            if_true: body,
            elifs: vec![],
            if_false: None,
        }
    }

    fn while_true_do<T>(self, body: T) -> WhileLoop<Self, T>
    where
        Self: Generate + Sized,
    {
        WhileLoop {
            condition: self,
            body,
        }
    }

    fn get_element<T>(self, index: T) -> GetElement<Self, T>
    where
        Self: Generate + Sized,
    {
        GetElement {
            object: self,
            index,
        }
    }

    fn set_element<T, V>(self, index: T, value: V) -> SetElement<Self, T, V>
    where
        Self: Generate + Sized,
    {
        SetElement {
            object: self,
            index,
            value,
        }
    }
}

#[derive(Copy, Clone)]
pub struct Variable<'n> {
    name: &'n str,
    depth: Option<(usize, usize)>,
}

impl<'n> Debug for Variable<'n> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.name)
    }
}

impl<'n> Variable<'n> {
    pub fn global(name: impl Into<&'n str>) -> Self {
        Variable {
            name: name.into(),
            depth: None,
        }
    }

    pub fn local(name: impl Into<&'n str>, depth: (usize, usize)) -> Self {
        Variable {
            name: name.into(),
            depth: Some(depth),
        }
    }

    pub fn binding(&self) -> Binding {
        match self.depth {
            None => Binding::global(&self.name),
            Some((depth, function_depth)) => Binding::local(&self.name, depth, function_depth),
        }
    }

    pub fn bind<T>(&self, value: T) -> Assignement<T>
    where
        T: Generate,
    {
        Assignement {
            variable: self.clone(),
            value,
        }
    }
}

impl<'n> Generate for Variable<'n> {
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let type_info = context
            .types
            .last()
            .unwrap()
            .get(&self.name.to_string())
            .unwrap_or(&TypeInfo::nil())
            .clone();

        match self.depth {
            None => Expr::Var(Binding::global(&self.name)).node(type_info),
            Some((depth, function_depth)) => {
                Expr::Var(Binding::local(&self.name, depth, function_depth)).node(type_info)
            }
        }
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::any()
    }
}

// To make the api nicer Assignement
// doesn't take a reference to the value
// this comes at the cost of not being
// able to clone the struct, this doesn't
// seem necessary but if you think so feel
// free to open an issue
#[derive(Debug)]
pub struct Assignement<'n, T> {
    variable: Variable<'n>,
    value: T,
}

impl<'n, T> Generate for Assignement<'n, T>
where
    T: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let binding = self.variable.binding();

        let type_info = self.value.type_info(&context).clone();

        let map = context
            .types
            .get_mut(binding.clone().depth.unwrap_or(0) + binding.function_depth)
            .unwrap();

        map.insert(binding.name().into(), type_info);

        Expr::Bind(binding.clone(), self.value.generate(context)).node(TypeInfo::nil())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::nil()
    }
}

#[derive(Debug)]
pub struct Call<T> {
    callee: T,
    args: Vec<Box<dyn Generate>>,
}

impl<T> Call<T> {
    pub fn new(callee: T, args: Vec<Box<dyn Generate>>) -> Self {
        Self { callee, args }
    }
}

impl<T> Generate for Call<T>
where
    T: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let callee = self.callee.generate(context);
        let args = self
            .args
            .iter()
            .map(|elem| elem.generate(context))
            .collect();

        Expr::Call(super::ir::Call { callee, args }).node(self.callee.type_info(context))
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        self.callee.type_info(context)
    }
}

#[derive(Debug)]
pub struct Return<T> {
    value: T,
}

impl<T> Return<T> {
    pub fn new(value: T) -> Self {
        Self { value }
    }
}

impl<T> Generate for Return<T>
where
    T: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let type_info = self.value.type_info(context);
        Expr::Return(Some(self.value.generate(context))).node(type_info)
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        self.value.type_info(context)
    }
}

pub struct Function<'n, T> {
    variable: Variable<'n>,
    args: Vec<String>,
    body: T,
}

impl<'n, T> Function<'n, T> {
    /// Body must contain at least one return for the function to be valid.  
    /// As such, the body cannot be empty, or else zub will try to read the
    /// next instruction of the function, which does not exist.
    /// If you use some similar to Function::new(..., ..., {...; some_node}) be careful
    /// to not add a trailing `;` which is an easy mistake to make (the error happens during)
    /// execution which makes it hard to diagnose.  
    /// TODO: Possible automatic return insertion/warning if missing?
    pub fn new<'a>(variable: Variable<'n>, args: Vec<&'a str>, body: T) -> Self
    where
        T: Generate,
    {
        Self {
            variable,
            args: args.iter().map(|it| it.to_string()).collect(),
            body,
        }
    }
}

impl<'n, T> Debug for Function<'n, T> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Function")
            .field("variable", &self.variable)
            .field("args", &self.args)
            .field("function", &"<...>")
            .finish()
    }
}

impl<'n, T> Generate for Function<'n, T>
where
    T: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let mut body_builder = IrBuilder::new();
        self.body.emit(&mut body_builder);

        context.scope_in();
        let body = body_builder.build();
        context.scope_out();

        let func_body = IrFunctionBody {
            params: self
                .args
                .iter()
                .cloned()
                .map(|x| {
                    Binding::local(
                        &x,
                        self.variable.binding().depth.unwrap_or(0) + 1,
                        self.variable.binding().function_depth + 1,
                    )
                })
                .collect::<Vec<Binding>>(),
            method: false,
            inner: body,
        };

        let ir_func = IrFunction {
            var: self.variable.binding(),
            body: Rc::new(RefCell::new(func_body)),
        };

        Expr::Function(ir_func).node(TypeInfo::any())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::nil()
    }
}

/// Structure to represent a conditional statement/expression.
/// `if_false` needs to be boxed
pub struct Conditional<'a, C, T, E> {
    condition: C,
    if_true: T,
    elifs: Vec<(Box<dyn Generate + 'a>, Box<dyn Generate + 'a>)>,
    if_false: Option<E>,
}

impl<'a, C, T, E> Conditional<'a, C, T, E> {
    /// Sets the else clause of that conditional. `else_body` needs
    /// to be boxed.
    pub fn else_do(mut self, else_body: E) -> Self {
        self.if_false = Some(else_body);
        self
    }

    pub fn else_if<A, B>(mut self, condition: A, body: B) -> Self
    where
        A: Generate + 'a,
        B: Generate + 'a,
    {
        let tuple: (Box<dyn Generate>, Box<dyn Generate>) = (condition.boxed(), body.boxed());
        self.elifs.push(tuple);
        self
    }
}

impl<'a, C, T> Conditional<'a, C, T, ()> {
    pub fn else_none(mut self) -> Self {
        self.if_false = Option::<()>::None;
        self
    }
}

impl<'a, C, T, E> Debug for Conditional<'a, C, T, E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Conditional")
            .field("condition", &"<...>")
            .field("if_true", &"{...}")
            .field("if_false", &"{...}?")
            .finish()
    }
}

impl<'a, C, T, E> Generate for Conditional<'a, C, T, E>
where
    C: Generate,
    T: Generate,
    E: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let tr = self.if_true.generate(context);
        let fl = if let Some(ref x) = self.if_false {
            Some(x.generate(context))
        } else {
            None
        };

        let elifs = self
            .elifs
            .iter()
            .map(|(x, y)| (x.generate(context), y.generate(context)))
            .collect();

        Expr::If(self.condition.generate(context), tr, elifs, fl).node(TypeInfo::nil())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        let if_true_ty = self.if_true.type_info(context);
        let if_false_ty = if let Some(ref x) = self.if_false {
            x.type_info(context)
        } else {
            if_true_ty.clone()
        };

        if if_true_ty == if_false_ty {
            return if_false_ty;
        }

        TypeInfo::any()
    }
}

pub struct WhileLoop<C, B> {
    condition: C,
    body: B,
}

impl<C, B> Debug for WhileLoop<C, B>
where
    C: Generate,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "while {:?} {{ ... }}", self.condition)
    }
}

impl<C, B> Generate for WhileLoop<C, B>
where
    C: Generate,
    B: Fn(&mut IrBuilder),
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let mut body_builder = IrBuilder::new();
        (self.body)(&mut body_builder);

        let body = Expr::Block(body_builder.build()).node(TypeInfo::nil());

        Expr::While(self.condition.generate(context), body).node(TypeInfo::nil())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::nil()
    }
}

pub struct BinaryOperation<L, R> {
    lhs: L,
    rhs: R,
    operator: BinaryOp,
}

impl<L, R> Debug for BinaryOperation<L, R>
where
    L: Debug,
    R: Debug,
{
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write(
            f,
            format_args!(
                "{:?} {} {:?}",
                self.lhs,
                self.operator.as_symbol(),
                self.rhs
            ),
        )
    }
}

impl<L, R> Generate for BinaryOperation<L, R>
where
    L: Generate,
    R: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        let lhs_ty = self.lhs.type_info(context);
        let rhs_ty = self.rhs.type_info(context);

        if !lhs_ty.is_numerical() || !rhs_ty.is_numerical() {
            let message = format!(
                "Trying to apply '{}' on types of {:?} and {:?} => {:?} {} {:?}",
                self.operator.as_symbol(),
                lhs_ty.kind(),
                rhs_ty.kind(),
                self.lhs,
                self.operator.as_symbol(),
                self.rhs
            );
            warn!("{}", message);
        }

        let lhs = self.lhs.generate(context);
        let rhs = self.rhs.generate(context);

        context.binary(lhs, self.operator.clone(), rhs)
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        self.lhs.type_info(context)
    }
}

#[derive(Debug, Clone)]
pub struct GetElement<O, I> {
    object: O,
    index: I,
}

impl<O, I> Generate for GetElement<O, I>
where
    O: Generate,
    I: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        Expr::GetElement(self.object.generate(context), self.index.generate(context))
            .node(TypeInfo::any())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::any()
    }
}

#[derive(Debug, Clone)]
pub struct SetElement<O, I, V> {
    object: O,
    index: I,
    value: V,
}

impl<O, I, V> Generate for SetElement<O, I, V>
where
    O: Generate,
    I: Generate,
    V: Generate,
{
    fn generate(&self, context: &mut IrBuilder) -> ExprNode {
        Expr::SetElement(
            self.object.generate(context),
            self.index.generate(context),
            self.value.generate(context),
        )
        .node(TypeInfo::any())
    }

    fn type_info(&self, context: &IrBuilder) -> TypeInfo {
        TypeInfo::nil()
    }
}

macro_rules! impl_operations {
    ($target:tt$(< $($lt:tt),+ >)? => $imp:tt) => {
        impl_operation_generic!($target$(<$($lt),+>)?, $imp);
    };
}

macro_rules! impl_operation_generic {
    ($target:tt$(< $($lt:tt),+ >)?, Numerical) => {
        impl_operation_generic!($target$(<$($lt),+>)?, +);
        impl_operation_generic!($target$(<$($lt),+>)?, -);
        impl_operation_generic!($target$(<$($lt),+>)?, *);
        impl_operation_generic!($target$(<$($lt),+>)?, /);
        impl_operation_generic!($target$(<$($lt),+>)?, %);
    };

    ($target:tt$(< $($lt:tt),+ >)?, PartialEq) => {
        impl_partial_cmp!($target$(<$($lt),+>)?, equals, BinaryOp::Equal);
        impl_partial_cmp!($target$(<$($lt),+>)?, not_equals, BinaryOp::NEqual);
    };

    ($target:tt$(< $($lt:tt),+ >)?, BoolOperations) => {
        impl_partial_cmp!($target$(<$($lt),+>)?, and, BinaryOp::And);
        impl_partial_cmp!($target$(<$($lt),+>)?, or, BinaryOp::Or);
    };

    ($target:ident$(< $($lt:tt),+ >)?, PartialOrd) => {
        impl_partial_cmp!($target$(<$($lt),+>)?, lt, BinaryOp::Lt);
        impl_partial_cmp!($target$(<$($lt),+>)?, lte, BinaryOp::LtEqual);
        impl_partial_cmp!($target$(<$($lt),+>)?, gt, BinaryOp::Gt);
        impl_partial_cmp!($target$(<$($lt),+>)?, gte, BinaryOp::GtEqual);
    };

    ($target:tt$(< $($lt:tt),+ >)?, +) => {
        impl_operation!($target$(<$($lt),+>)?, add, Add, Add);
    };
    ($target:tt$(< $($lt:tt),+ >)?, -) => {
        impl_operation!($target$(<$($lt),+>)?, sub, Sub, Sub);
    };
    ($target:tt$(< $($lt:tt),+ >)?, *) => {
        impl_operation!($target$(<$($lt),+>)?, mul, Mul, Mul);
    };
    ($target:tt$(< $($lt:tt),+ >)?, /) => {
        impl_operation!($target$(<$($lt),+>)?, div, Div, Div);
    };
    ($target:tt$(< $($lt:tt),+ >)?, %) => {
        impl_operation!($target$(<$($lt),+>)?, rem, Rem, Rem);
    };
}

macro_rules! impl_partial_cmp {
    ($target:tt$(< $($lt:tt),+ >)?, $method_name:ident, $bin_op:expr) => {
        impl$(<$($lt),+>)? $target$(<$($lt),+>)? {
            /// Compares two implementors of the Generate trait
            pub fn $method_name<R>(self, other: R) -> BinaryOperation<Self, R>
            where
                Self: Generate,
            {
                BinaryOperation {
                    lhs: self,
                    operator: $bin_op,
                    rhs: other,
                }
            }
        }
    };
}

macro_rules! impl_operation {
    ($target:tt$(< $($lt:tt),+ >)?, $method_name:tt, $trait_name:ident, $bin_op_variant:tt) => {
        impl<$($($lt),+)?, R> $trait_name<R> for $target$(<$($lt),+>)?
        where
            R: Generate,
        {
            type Output = BinaryOperation<$target$(<$($lt),+>)?, R>;

            fn $method_name(self, rhs: R) -> Self::Output {
                BinaryOperation {
                    lhs: self,
                    rhs,
                    operator: BinaryOp::$bin_op_variant,
                }
            }
        }
    };
}

impl_operations!(Variable<'a> => PartialEq);
impl_operations!(Variable<'a> => PartialOrd);
impl_operations!(Variable<'a> => Numerical);
impl_operations!(Variable<'a> => BoolOperations);

impl_operations!(BinaryOperation<A, B> => PartialEq);
impl_operations!(BinaryOperation<A, B> => PartialOrd);
impl_operations!(BinaryOperation<A, B> => Numerical);
impl_operations!(BinaryOperation<A, B> => BoolOperations);

impl_operations!(Call<A> => PartialEq);
impl_operations!(Call<A> => PartialOrd);
impl_operations!(Call<A> => Numerical);
impl_operations!(Call<A> => BoolOperations);
