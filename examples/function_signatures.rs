use mlua_extras::{extras::Module, typed::{Type, TypedModule, TypedModuleMethods}};

struct MyModule;
impl TypedModule for MyModule {
    fn add_methods<'lua, M: TypedModuleMethods<'lua>>(methods: &mut M) -> mlua::Result<()> {
        methods
            .document("A function with a robust signature")
            .add_function_with(
                "signature",
                |_lua, params: (f32, bool, i32, String, [f32; 4])| {
                    println!("Function got parameters : {params:#?}");
                    Ok(())
                },
                |func| {
                    func.param(0, |param| param.name("p_number").doc("Some number").ty(Type::number()));
                    func.param(1, |param| param.name("p_bool").doc("Some boolean").ty(Type::boolean()));
                    func.param(2, |param| param.name("p_integer").doc("Somer integer").ty(Type::integer()));
                    func.param(3, |param| param.name("p_string").doc("Some string").ty(Type::string()));
                    func.param(4, |param| param.name("p_vec4").doc("A four value tuple of numbers, effectively a Vector3")
                        .ty(Type::tuple([Type::number(),Type::number(),Type::number()])));
                })
    }
}

fn main() -> mlua::Result<()> {
    let lua = mlua::Lua::new();
    lua.globals().set("my_module", MyModule::module())?;
    if let Err(err) = lua.load(r#"
    my_module.signature(0.0, false, 0, "", {0.0, 0.0, 0.0, 0.0})
    my_module.signature(32.0, true, 45, "Hello, world!", {1.0, 3.0, -5.0, 0.0})    
"#).eval::<mlua::Value>() {
        eprintln!("{err}");
    }
    Ok(())
}
