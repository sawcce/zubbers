// #![feature(vec_drain_as_slice)]

extern crate flame;
#[macro_use]
extern crate flamer;
extern crate im_rc;

pub mod compiler;
pub mod ir;
pub mod vm;
extern crate env_logger;

#[cfg(test)]
mod tests {
    use std::vec;

    use colored::Colorize;

    use super::ir::*;
    use super::vm::*;

    #[test]
    fn globals() {
        let mut builder = IrBuilder::new();

        let value = builder.number(42.0);
        builder.bind(Binding::global("foo"), value);

        let mut vm = VM::new();

        vm.exec(&builder.build(), true);

        println!("{:#?}", vm.globals)
    }

    #[test]
    fn locals() {
        let mut builder = IrBuilder::new();

        let value = builder.number(42.0);
        builder.bind(Binding::local("foo", 0, 0), value);

        builder.bind(
            Binding::global("FOO"),
            builder.var(Binding::local("foo", 0, 0)),
        );

        let mut vm = VM::new();

        vm.exec(&builder.build(), true);

        println!("{:#?}", vm.globals)
    }

    #[test]
    fn binary() {
        let mut builder = IrBuilder::new();

        let a = builder.number(20.0);
        let b = builder.number(30.0);

        let sum = builder.binary(a, BinaryOp::Add, b);

        builder.bind(Binding::global("sum"), sum);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!("{:#?}", vm.globals)
    }

    #[test]
    fn get_element() {
        let mut builder = IrBuilder::new();
        let list = builder.list(vec![
            builder.number(0f64),
            builder.number(10f64),
            builder.number(89f64),
        ]);
        builder.bind(Binding::global("list"), list);

        let var = builder.var(Binding::global("list"));
        let element = builder.get_element(var, builder.number(1f64));
        builder.bind(Binding::global("result"), element);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);
        println!("{:#?}", vm.globals)
    }

    #[test]
    fn actual_real_functions() {
        /*
            function foo(a, b) {
                return a + b
            }

            global bar = foo(10.0, 30.0)
        */

        let mut builder = IrBuilder::new();

        let foo = builder.function(Binding::local("foo", 0, 0), &["a", "b"], |builder| {
            let a = builder.var(Binding::local("a", 1, 1));
            let b = builder.var(Binding::local("b", 1, 1));

            let sum = builder.binary(a, BinaryOp::Add, b);

            builder.ret(Some(sum))
        });

        builder.emit(foo);

        let args = vec![builder.number(10.0), builder.number(30.0)];

        let callee = builder.var(Binding::local("foo", 0, 0));
        let call = builder.call(callee, args, None);

        builder.bind(Binding::global("bar"), call); // assign "bar" to call here

        let built = builder.build();

        let mut vm = VM::new();
        vm.exec(&built, true);

        println!("{:#?}", vm.globals)
    }

    #[test]
    fn ffi() {
        let mut builder = IrBuilder::new();

        let hello = Expr::Literal(Literal::String("Hello from Rust :D".to_string()))
            .node(TypeInfo::new(Type::String));

        let callee = builder.var(Binding::global("print"));

        let call = builder.call(callee, vec![hello], None);

        builder.emit(call);

        let mut vm = VM::new();

        vm.add_native("print", print_native, 1);
        vm.exec(&builder.build(), true);
    }

    #[test]
    fn tuple() {
        let mut builder = IrBuilder::new();

        let members = vec![
            builder.number(11.0),
            builder.number(22.0),
            builder.number(33.0),
        ];

        let tuple = builder.tuple(members);
        builder.bind(Binding::global("tuple"), tuple.clone());

        let var = builder.var(Binding::global("tuple"));
        builder.bind(
            Binding::global("value"),
            builder.get_element(var.clone(), builder.number(0.0)),
        );

        let callee = builder.var(Binding::global("print"));
        let call = builder.call(callee, vec![builder.var(Binding::global("value"))], None);
        builder.emit(call);

        builder.emit(builder.set_element(var.clone(), builder.number(0.0), builder.number(18.0)));
        builder.bind(
            Binding::global("value"),
            builder.get_element(var, builder.number(0.0)),
        );

        let callee = builder.var(Binding::global("print"));
        let call = builder.call(callee, vec![builder.var(Binding::global("tuple"))], None);
        builder.emit(call);

        let mut vm = VM::new();
        vm.add_native("print", print_native, 1);
        vm.exec(&builder.build(), true);
        let a = vm.globals.get("tuple").unwrap().as_object().unwrap();
        let a = unsafe { a.get_unchecked() };
        println!("A: {:#?}", a)
    }

    #[test]
    fn structure() {
        let mut builder = IrBuilder::new();

        let members = vec![builder.string("Sawcce"), builder.number(16.0)];

        let keys = vec!["name".into(), "age".into()];

        let structure = builder.structure(keys, members);
        builder.bind(Binding::global("struct"), structure.clone());
        let var = builder.var(Binding::global("struct"));

        builder.bind(
            Binding::global("value"),
            builder.get_member("name".into(), var.clone()),
        );

        let callee = builder.var(Binding::global("print"));
        let call = builder.call(
            callee.clone(),
            vec![builder.var(Binding::global("value"))],
            None,
        );
        builder.emit(call);

        let b = builder.set_member("name".into(), var.clone(), builder.string("Hello, world!"));
        builder.emit(b);

        builder.bind(
            Binding::global("value"),
            builder.get_member("name".into(), var),
        );

        let call = builder.call(callee, vec![builder.var(Binding::global("value"))], None);
        builder.emit(call);

        let mut vm = VM::new();
        vm.add_native("print", print_native, 1);
        vm.exec(&builder.build(), true);
        let a = vm.globals.get("struct").unwrap().as_object().unwrap();
        let a = unsafe { a.get_unchecked() };
        println!("A: {:#?}", a)
    }

    #[test]
    fn list() {
        let mut builder = IrBuilder::new();

        let content = vec![
            builder.number(11.0),
            builder.number(22.0),
            builder.number(33.0),
        ];

        let list = builder.list(content);

        builder.bind(Binding::local("bob", 0, 0), list);

        let var = builder.var(Binding::local("bob", 0, 0));

        let index = builder.int(0);

        let new_element = builder.number(777.0);
        let set_list_element = builder.set_element(var.clone(), index.clone(), new_element);
        builder.emit(set_list_element);

        let right = builder.get_element(var, index);

        builder.bind(Binding::global("element"), right); // expect 777.0

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!("{:#?}", vm.globals)
    }

    #[test]
    /// This test is left as-is to showcase how upvalues work
    /// but it is much heasier to directly use a global binding
    /// for fib in that specific case.
    fn recursion() {
        let mut builder = IrBuilder::new();

        // This binding is used as the actual binding of the function fib, in the root scope.
        // The function is defined at a depth of 0, and a function depth of 0.
        let fib_binding = Binding::local("fib", 0, 0);

        let fib = builder.function(fib_binding.clone(), &["n"], |builder| {
            // This is where things get funky. Here we want to access fib from inside its own scope.
            // Thus it has to be made clear that we're upvalueing fib for this binding.
            // ... An *upvalue* is made when depth > function_depth
            // In conclusion, we're accessing fib from a depth of one, whereas fib is at function depth 0, 0
            let upvalue_fib = Binding::local("fib", 1, 0);

            // Here we're simply accessing acessing the parameter n, which will be bound at depth 1 and function_depth 1
            let n = builder.var(Binding::local("n", 1, 1));

            let one = builder.number(1.0);
            let two = builder.number(2.0);

            let binary_0 = builder.binary(n.clone(), BinaryOp::Sub, one);
            let binary_1 = builder.binary(n.clone(), BinaryOp::Sub, two.clone());

            println!("{}", upvalue_fib.is_upvalue());

            // Here we're generating a reference based on the upvalue binding
            // This is used inside this scope, and will be cloned a couple of times.
            let fib_var = builder.var(upvalue_fib.clone()); // Fine for now, always pointing in the right direction :D

            let call_0 = builder.call(fib_var.clone(), vec![binary_0], None);
            let call_1 = builder.call(fib_var, vec![binary_1], None);

            let final_binary = builder.binary(call_0, BinaryOp::Add, call_1);

            let n_less_than_2 = builder.binary(n.clone(), BinaryOp::Lt, two);
            let ternary = builder.ternary(n_less_than_2, n.clone(), Some(final_binary));

            builder.ret(Some(ternary))
        });

        // We don't have to bind fib as this is already done during function compilation.
        // In the future, anonymous functions will be easier to make. :D
        builder.emit(fib);

        let ten = builder.number(10.0);
        let fib_var = builder.var(fib_binding);

        let fib_call = builder.call(fib_var, vec![ten], None);

        let print = builder.var(Binding::global("print"));
        let call = builder.call(print, vec![fib_call], None);

        builder.emit(call); // :D

        let mut vm = VM::new();
        vm.add_native("print", print_native, 1);
        vm.exec(&builder.build(), true);
    }

    #[test]
    fn dict() {
        let mut builder = IrBuilder::new();

        let fruit = builder.string("fruit");
        let apple = builder.string("Æble");

        let dict = builder.empty_dict();
        builder.bind(Binding::local("stuff", 0, 0), dict);

        let var = builder.var(Binding::local("stuff", 0, 0));

        let set_fruit = builder.set_element(var.clone(), fruit.clone(), apple);

        builder.emit(set_fruit);

        let get_fruit = builder.get_element(var.clone(), fruit);

        builder.bind(Binding::global("test"), get_fruit);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!(" sad sad {:#?}", vm.globals)
    }

    fn print_native(context: &mut CallContext) -> Value {
        println!("Print native: {}", context.get_arg_with_heap(1));
        Value::nil()
    }

    #[test]
    fn old_api() {
        let mut builder = IrBuilder::new();

        let pi_approx = 3.141592;
        let pi_binding = Binding::global("pi");
        builder.bind(pi_binding, builder.number(pi_approx));

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        assert_eq!(vm.globals.get("pi").unwrap().as_float(), pi_approx);
    }

    #[test]
    fn pi_math() {
        let mut builder = IrBuilder::new();

        // New api
        // In the new api, nothing is generated unless specified explicitely
        let pi = Variable::global("pi");
        pi.bind(3.141592).emit(&mut builder);

        // Old api
        // builder.bind(Binding::global("pi"), builder.number(3.141592));

        let deg_to_rad = Variable::global("deg_to_rad");

        Function::new(deg_to_rad, vec!["deg"], {
            let deg = Variable::local("deg", (1, 1));

            (deg / 360 * 2 * pi).ret()
        })
        .emit(&mut builder);

        Variable::global("60deg->rad")
            .bind(deg_to_rad.call(vec![60.boxed()]))
            .emit(&mut builder);
        Variable::global("180deg->rad")
            .bind(deg_to_rad.call(vec![180.boxed()]))
            .emit(&mut builder);

        let is_2pi = Variable::global("is_2pi");
        Function::new(is_2pi, vec!["angle"], {
            let angle = Variable::local("angle", (1, 1));
            let pi = Variable::global("pi");

            (angle % pi * 2)
                .equals(0)
                .if_true_do(true)
                .else_do(false)
                .ret()
        })
        .emit(&mut builder);

        Variable::global("is_2pi_2pi")
            .bind(is_2pi.call(vec![(pi * 2).boxed()]))
            .emit(&mut builder);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!("Globals: {:?}", vm.globals);

        assert_eq!(vm.globals.get("pi").unwrap().as_float(), 3.141592);
    }

    #[test]
    fn squared() {
        let mut builder = IrBuilder::new();

        let square = Variable::global("square");

        let square_def = Function::new(square, vec!["n"], {
            let n = Variable::local("n", (1, 1));

            (n * n).ret()
        });

        square_def.emit(&mut builder);

        let square_10 = square.call(vec![10.boxed()]);
        Variable::global("10²").bind(square_10).emit(&mut builder);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!("Globals: {:#?}", vm.globals);
    }

    #[test]
    fn type_safety() {
        env_logger::init();

        let mut builder = IrBuilder::new();

        let x = Variable::global("x");
        x.bind(10).emit(&mut builder);

        Variable::global("y").bind(x + Some(10)).emit(&mut builder);
        // These would yield type error warnings
        // Variable::global("y")
        //     .bind(x + Option::<()>::None)
        //     .emit(&mut builder);
        // Variable::global("y").bind(x + ()).emit(&mut builder);

        let mut vm = VM::new();
        vm.exec(&builder.build(), true);

        println!("Globals: {:#?}", vm.globals);
    }

    #[test]
    fn fizz_buzz() {
        let mut builder = IrBuilder::new();

        let n = Variable::global("n");
        n.bind(0).emit(&mut builder);

        let print = Variable::global("print");

        n.lte(100)
            .while_true_do(|builder: &mut IrBuilder| {
                (n % 3)
                    .equals(0)
                    .and((n % 5).equals(0))
                    .if_true_do(print.call(vec!["FizzBuzz!".to_string().boxed()]))
                    .else_if(
                        (n % 3).equals(0),
                        print.call(vec!["Fizz!".to_string().boxed()]),
                    )
                    .else_if(
                        (n % 5).equals(0),
                        print.call(vec!["Buzz!".to_string().boxed()]),
                    )
                    .else_do(print.call(vec!["".to_string().boxed()]))
                    .emit(builder);

                n.bind(n + 1).emit(builder);
            })
            .emit(&mut builder);

        let mut vm = VM::new();
        vm.add_native("print", print_native, 1);
        vm.exec(&builder.build(), true);
    }
}
