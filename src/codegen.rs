
use llvm::*;
use parser::ASTNode;
use subprogram::{Subprogram, Declaration};
use prototype::*;
use data_type::*;
use statement::Statement;
use expression::Expression;
use std::collections::HashMap;

pub struct CodegenContext<'a> {
    pub ctx: &'a Context,
    pub module: &'a Module,
    pub fmap: FunctionMap<'a>,
}

struct Variable<'a> {
    pub value: &'a Value,
    pub is_pointer: bool,
}

pub struct FunctionMeta<'a> {
    pub function: &'a Function,
    pub prototype: Prototype,
}

struct SubprogramContext<'a> {
    pub vars: VariableMap<'a>,
    pub func: &'a Function,
    pub builder: &'a Builder,
}

type VariableMap<'a> = HashMap<&'a str, Variable<'a>>;
type FunctionMap<'a> = HashMap<String, FunctionMeta<'a>>;

fn void_type<'a>() -> &'a Type {
    unsafe {
        use llvm_sys::core::*;
        use llvm_sys::prelude::LLVMTypeRef;
        let typeref: LLVMTypeRef = LLVMVoidType();
        typeref.into()
    }
}

fn pointer_type<'a>(t: &'a Type) -> &'a Type {
    unsafe {
        use llvm_sys::core::*;
        LLVMPointerType(t.into(), 0 as u32).into()
    }
}

impl<'a> CodegenContext<'a> {
    pub fn codegen(&mut self, ast_node: &ASTNode) {
        match *ast_node {
            ASTNode::Subprogram(ref sp) => self.codegen_subprogram(sp),
            ASTNode::Declaration(ref dec) => self.codegen_dec(dec),
        };
    }

    fn codegen_subprogram(&mut self, sp: &Subprogram) {
        let mut is_new = false;
        let func = match self.fmap.get(sp.prototype.name.as_str()) {
            Some(f) => f.function,
            None => {
                is_new = true;
                self.module.add_function(sp.prototype.name.as_str(),
                                         self.function_type_from_prototype(&sp.prototype))
            }
        };

        let entry = func.append("entry");
        let builder = Builder::new(self.ctx);
        builder.position_at_end(entry);

        if is_new {
            self.fmap.insert(sp.prototype.name.clone(),
                             FunctionMeta {
                                 function: func,
                                 prototype: sp.prototype.clone(),
                             });
        }

        // Create the variable list.
        let mut variables: VariableMap = HashMap::new();

        let mut arg_counter: usize = 0;
        for param in &sp.prototype.arguments {
            variables.insert(param.name.as_str(),
                             Variable {
                                 value: &func[arg_counter],
                                 is_pointer: param.direction != ParameterDirection::In,
                             });
            arg_counter += 1;
        }

        for var_decl in &sp.variables {
            variables.insert(var_decl.name.as_str(),
                        Variable {
                            value: builder.build_alloca(self.codegen_datatype(&var_decl.data_type, false)),
                            is_pointer: true,
                        });
        }

        let subprogram_ctx = SubprogramContext {
            vars: variables,
            func: func,
            builder: &builder,
        };

        let mut has_returned = false;
        for s in &sp.body {
            if has_returned {
                panic!("Dead code after a return statement!");
            }
            self.codegen_statement(s, &mut has_returned, &subprogram_ctx);
        }


        if !has_returned {
            // Add a return void so that procedures don't need to end with
            // a "return;"
            builder.build_ret_void();
        }
    }

    fn codegen_dec(&mut self, dec: &Declaration) {
        let func = self.module
            .add_function(dec.prototype.name.as_str(),
                          self.function_type_from_prototype(&dec.prototype));
        self.fmap.insert(dec.prototype.name.clone(),
                         FunctionMeta {
                             function: func,
                             prototype: dec.prototype.clone(),
                         });
    }

    fn codegen_call(&self,
                    name: &String,
                    parameters: &[Expression],
                    sctx: &'a SubprogramContext)
                    -> &'a Value {
        // Find the function.
        let func = &self.fmap[name];
        let mut args: Vec<&'a Value> = vec![];
        let mut par_index: usize = 0;
        for arg in &func.prototype.arguments {
            let is_pointer = arg.direction != ParameterDirection::In;
            match is_pointer {
                false => args.push(self.codegen_expression(&parameters[par_index], sctx)),
                true => {
                    match parameters[par_index] {
                        Expression::Variable(ref s) => {
                            /// @todo Should check if the variable is indeed a pointer.
                            args.push(sctx.vars[s.as_str()].value)
                        }
                        _ => panic!("\"out\" parameters need an identifier!"),
                    }
                }
            };
            par_index += 1;
        }
        sctx.builder.build_call(func.function, args.as_slice())
    }

    fn codegen_statement(&self,
                         s: &Statement,
                         has_returned: &mut bool,
                         sctx: &'a SubprogramContext) {
        match *s {
            Statement::Return(ref val) => {
                *has_returned = true;
                match *val {
                    Some(ref exp) => {
                        sctx.builder
                            .build_ret(self.codegen_expression(exp, sctx))
                    }
                    None => sctx.builder.build_ret_void(),
                }
            }
            Statement::Assignment(ref dest, ref val) => {
                if sctx.vars[dest.as_str()].is_pointer == false {
                    panic!("Attempting to assign to an argument marked as \"in\".");
                }
                sctx.builder.build_store(self.codegen_expression(val, sctx),
                                         &sctx.vars[dest.as_str()].value)
            }
            Statement::ProcedureCall(ref s, ref args) => {
                self.codegen_call(s, args.as_slice(), sctx)
            }
        };
    }

    fn codegen_expression(&self, e: &Expression, sctx: &'a SubprogramContext) -> &'a Value {
        match *e {
            Expression::IntValue(x) => x.compile(self.ctx),
            Expression::FloatValue(x) => x.compile(self.ctx),
            Expression::Variable(ref name) => {
                let var_info = &sctx.vars[name.as_str()];
                match var_info.is_pointer {
                    true => sctx.builder.build_load(var_info.value),
                    false => var_info.value,
                }
            }
            Expression::Binary(ref op, ref lhs, ref rhs) => {
                let v1 = self.codegen_expression(lhs.as_ref(), sctx);
                let v2 = self.codegen_expression(rhs.as_ref(), sctx);
                match op.as_str() {
                    "+" => sctx.builder.build_add(v1, v2),
                    "-" => sctx.builder.build_sub(v1, v2),
                    "*" => sctx.builder.build_mul(v1, v2),
                    "/" => sctx.builder.build_div(v1, v2),
                    _ => unimplemented!(),
                }
            }
            Expression::FunctionCall(ref s, ref args) => {
                self.codegen_call(s, args.as_slice(), sctx)
            }
        }
    }

    fn codegen_datatype(&self, dt: &DataType, is_pointer: bool) -> &'a Type {
        match is_pointer {
            true => pointer_type(self.codegen_datatype(dt, false)),
            false => {
                match *dt {
                    DataType::Float => Type::get::<f32>(self.ctx),
                    DataType::Integer => Type::get::<i32>(self.ctx),
                }
            }
        }
    }

    fn function_type_from_prototype(&self, p: &Prototype) -> &'a FunctionType {
        let return_type = match p.returns {
            Some(ref dtype) => self.codegen_datatype(&dtype, false),
            None => void_type(), 
        };
        let mut arg_types: Vec<&Type> = vec![];
        for arg_decl in &p.arguments {
            arg_types.push(self.codegen_datatype(&arg_decl.data_type,
                                                 arg_decl.direction != ParameterDirection::In));
        }
        FunctionType::new(return_type, &arg_types[..])
    }
}
