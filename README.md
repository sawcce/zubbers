# Zub VM
> A super-fast, stack-based virtual machine for dynamic languages

# Warning
This library has been forked for a while but updates have been slow
and inconsistent due to my studies. The project is not abandoned, though
the usability may not be ideal and I can't guarantee a bug-free experience.
Feel free to make PRs or issues if you encounter any bugs, I'll do my best
to review them.

## Features

- NaN-tagging value representation
- Mark n' sweep garbage collection
- Compact bytecode format
- Easy-to-use intermediate representation

## Milestones

- [x] Refined VM based on work by [Mr Briones](https://github.com/cwbriones)
- [x] Tracing garbage collector
- [x] High-level IR
- [x] Compilation of IR
- [ ] Optimizer (currently 80-90% Python speed, aiming for much faster)
- [x] Profiler and disassembler

## Example

### Building IR is easy (yes, but)

Getting your backend up and running shouldn't have to be hard.

The following code builds IR for evaluating `sum = 20.0 + 30.0`:

```rust
let mut builder = IrBuilder::new();

let a = builder.number(20.0);
let b = builder.number(30.0);

let sum = builder.binary(a, BinaryOp::Add, b);

builder.bind(Binding::global("sum"), sum);
```

When you feel like the IR is looking smooth. Simply let VM throw it through the compiler, and run it.

```rust
let mut vm = VM::new();
vm.exec(&builder.build());
```

### (yest, but) there's a new interface
In order to provide a smoother experience, a new experimental `v2` IR Builder pattern is being worked on.
The main idea is that your IR generation pipeline depends much less on `IrBuilder`.
This new api has been in the works due to the fact that multiple methods like `builder.number` shouldn't
depend on the builder to work (and their implementation doesn't); this new api aims to take full advantage
of rust traits so that you don't have to worry about all of the technicalities.  
To do so, you use some existing structs/methods provided such as `Function::new`, `Variable::global,local` etc...
the key difference is that nothing is added to the program unless explicitely said so.  
**Enough talk, let's see that sweet sweet example!**

```rust
let mut builder = IrBuilder::new();

Variable::global("sum")
  .bind(20 + 30)
  .emit(&mut builder);
```
You might be asking yourself "but wouldn't that mean that 20 + 30 is removed at rust compile time?", the answer is y.e.s. This new
interface prevents you from doing simple mistakes like these.
As you can see `emit` is explicitely called in order to add the assignement to the program! 

## V2
So... what's the big deal with the V2 bindings?   
The main advantage is that your program structure is valid without having the need to rely on `IrBuilder`. This allows you to copy variable references and so much more. It's like having an AST that you can then add to your program in whatever order you want.

### How
This new interface relies on one single trait:

```rust
pub trait Generate: Debug {
    fn generate(&self, context: &mut IrBuilder) -> ExprNode;
    fn type_info(&self, context: &IrBuilder) -> TypeInfo;
}
``` 

each structure that you want to enable to generate IR must implement that trait. This gives us the ability to multiply structs together using rust's own traits:

```rust
let square_def = Function::new(square.clone(), vec!["n"], |builder| {
    let n = Variable::local("n", (1, 1));

    (n.clone() * n).ret().emit(builder); // We can multiply n by itself
});
```

The trait also provides some helpers (as demonstrated before):
```rust
fn ret(self) -> Return<Self>
where
    Self: Generate + Sized,
{
    Return { value: self }
}
```

If you want to create your own struct to facilitate the creation of classes or practically anything, you can do it.

There are also macros to implement `*, +, /, %, .gt(), .lte()` operations on structs that implement generate:
```rust
impl_operations!(Variable<> => PartialEq); // == and =/=
impl_operations!(Variable<> => PartialOrd); // >=, <=, >, <
impl_operations!(Variable<> => Numerical); // +, -, *, /, %

impl_operations!(BinaryOperation<A, B> => PartialEq);
impl_operations!(BinaryOperation<A, B> => PartialOrd);
impl_operations!(BinaryOperation<A, B> => Numerical);
```

### Legacy support
You are free to use the "old" api and I plan on supporting it for zub's current set of features. If other features were to be added I can't guarantee if they would be supported by the old api. Please note that `v2` and the legacy api aren't mutually exclusive and you can technically mix-and-match both (although not recommended as the paradigms are widely different).

### Fancying more?
Feel free to take a look at `src/lib.rs` where there are many more example of the legacy api and some of `v2`.

## Languages

### Hugorm

Hugorm is a dynamic, python-like language being built for small data science and game projects.

[https://github.com/nilq/hugorm](https://github.com/nilq/Hugorm)

### Examples

The `examples/` folder includes two small language implementations running on the ZubVM.

#### Atto

Atto is a functional, minimal language that showcases how little code is needed to implement a working, Turing-complete language. The syntax can be seen in the following teaser:

```hs
fn sum x is
    if = x 0
        1
    + sum - x 1 sum - x 1

fn main is
    sum 12
```

#### Mini

Mini is a simple language that looks basically like a mix of Rust and JavaScript. It covers a bit wider set of features than Atto. This does show in the size of the language though.

```rust
let bar = 13.37;

fn foo() {
  fn baz(c) {
    return c + bar;
  }
  
  return baz(10);
}

global gangster = foo();
```


## Special thanks

- [zesterer](https://github.com/zesterer)
- [cwbriones](https://github.com/cwbriones)
- [evolbug](https://github.com/evolbug)
