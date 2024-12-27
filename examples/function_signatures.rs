use mlua_extras::{extras::Module, typed::{Type, TypedModule, TypedModuleMethods}};
/// Function Signatures Example
/// 
/// This example is a minimal module which has a long function signature using implementations of [Into<TypedMultiValue>] for (X, Y, Z, ....) where X, Y, Z, ... implement [Into<Typed>]

struct MyModule;
impl TypedModule for MyModule {
    fn add_methods<'lua, M: TypedModuleMethods<'lua>>(methods: &mut M) -> mlua::Result<()> {
        // Add a function with a robust signature
        methods
            .document("A function with a robust signature")
            .add_function_with(
                "signature",
                // note how `params` is a tuple type. This is because [TypedMultiValue] is implemented for basically any
                // tuple of [Typed] values. So your types can be anything as long as they can be represented in Lua as well
                |_lua, params: (f32, bool, i32, String, [f32; 4])| {
                    println!("Function got parameters : {params:?}");
                    Ok(())
                },
                |func| {
                    func.param(0, |param| param.name("p_number").doc("Some number").ty(Type::number()));
                    func.param(1, |param| param.name("p_bool").doc("Some boolean").ty(Type::boolean()));
                    func.param(2, |param| param.name("p_integer").doc("Somer integer").ty(Type::integer()));
                    func.param(3, |param| param.name("p_string").doc("Some string").ty(Type::string()));
                    func.param(4, |param| param.name("p_vec4").doc("A four value tuple of numbers, effectively a Vector3")
                        .ty(Type::tuple([Type::number(),Type::number(),Type::number()])));
                })?;
        // add a method that takes no parameters
        // Note that the `_params` is a unit tuple `()`. This denotes having no parameters
        methods
            .document("A function that takes no parameters")
            .add_function("print_hello", |_ctx, _params: ()| {
                println!("Hello world! (This was called with no function parameters)");
                Ok(())
            })?;

        Ok(())
    }
}

fn main() -> mlua::Result<()> {
    let lua = mlua::Lua::new();
    lua.globals().set("my_module", MyModule::module())?;
    if let Err(err) = lua.load(r#"
        -- in an IDE with LSP support, the function types would be shown along with the documentation
        my_module.signature(0.0, false, 0, "", {0.0, 0.0, 0.0, 0.0})
        my_module.signature(32.0, true, 45, "Hello, world!", {1.0, 3.0, -5.0, 0.0})
    
        my_module.print_hello()
    "#).eval::<mlua::Value>() {
        eprintln!("{err}");
    }
    Ok(())
}
