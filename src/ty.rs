use crate::{ast, reachability};
use anyhow::{Context, Result};
use std::collections::{HashMap, HashSet};

pub type ID = usize;

#[derive(Debug, Copy, Clone)]
pub struct Value(ID);

#[derive(Debug, Copy, Clone)]
pub struct Use(ID);

struct Bindings {
    m: HashMap<String, Value>,
    changes: Vec<(String, Option<Value>)>,
}

impl Bindings {
    fn new() -> Self {
        Self {
            m: HashMap::new(),
            changes: vec![],
        }
    }

    fn get(&self, k: &str) -> Option<Value> {
        self.m.get(k).copied()
    }

    fn insert(&mut self, k: String, v: Value) {
        self.m.insert(k.clone(), v);
    }

    fn in_child_scope<T>(&mut self, cb: impl FnOnce(&mut Self) -> T) -> T {
        let n = self.changes.len();
        let res = cb(self);
        self.unwind(n);
        res
    }

    fn unwind(&mut self, n: usize) {
        while self.changes.len() > n {
            let (k, old) = self.changes.pop().unwrap();
            match old {
                Some(v) => self.m.insert(k, v),
                None => self.m.remove(&k),
            };
        }
    }
}

#[derive(Debug, Clone)]
enum VTypeHead {
    VBool,
    VFunc { arg: Use, ret: Value },
    VObj { fields: HashMap<String, Value> },
    VCase { case: (String, Value) },
}

#[derive(Debug, Clone)]
enum UTypeHead {
    UBool,
    UFunc { arg: Value, ret: Use },
    UObj { field: (String, Use) },
    UCase { cases: HashMap<String, Use> },
}

fn check_heads(lhs: &VTypeHead, rhs: &UTypeHead, out: &mut Vec<(Value, Use)>) -> Result<()> {
    use UTypeHead::*;
    use VTypeHead::*;

    match (lhs, rhs) {
        (VBool, UBool) => Ok(()),
        (
            &VFunc {
                arg: arg1,
                ret: ret1,
            },
            &UFunc {
                arg: arg2,
                ret: ret2,
            },
        ) => {
            out.push((ret1, ret2));
            out.push((arg2, arg1));
            Ok(())
        }
        (VObj { fields }, UObj { field: (name, rhs) }) => match fields.get(name) {
            Some(lhs) => {
                out.push((*lhs, *rhs));
                Ok(())
            }
            None => bail!("Missing field: {}", name),
        },
        (VCase { case: (name, lhs) }, UCase { cases }) => match cases.get(name) {
            Some(rhs) => {
                out.push((*lhs, *rhs));
                Ok(())
            }
            None => bail!("Unhandled case: {}", name),
        },
        _ => bail!("Unexpected types"),
    }
}

#[derive(Debug, Clone)]
enum TypeNode {
    Var,
    Value(VTypeHead),
    Use(UTypeHead),
}

#[derive(Clone)]
pub struct TypeCheckerCore {
    r: reachability::Reachability,
    types: Vec<TypeNode>,
}

impl TypeCheckerCore {
    pub fn new() -> Self {
        Self {
            r: Default::default(),
            types: vec![],
        }
    }

    fn new_val(&mut self, val_type: VTypeHead) -> Value {
        let i = self.r.add_node();
        assert!(i == self.types.len());
        self.types.push(TypeNode::Value(val_type));
        Value(i)
    }

    fn new_use(&mut self, constraint: UTypeHead) -> Use {
        let i = self.r.add_node();
        assert!(i == self.types.len());
        self.types.push(TypeNode::Use(constraint));
        Use(i)
    }

    pub fn var(&mut self) -> (Value, Use) {
        let i = self.r.add_node();
        assert!(i == self.types.len());
        self.types.push(TypeNode::Var);
        (Value(i), Use(i))
    }

    fn bool(&mut self) -> Value {
        self.new_val(VTypeHead::VBool)
    }
    fn bool_use(&mut self) -> Use {
        self.new_use(UTypeHead::UBool)
    }

    fn func(&mut self, arg: Use, ret: Value) -> Value {
        self.new_val(VTypeHead::VFunc { arg, ret })
    }
    fn func_use(&mut self, arg: Value, ret: Use) -> Use {
        self.new_use(UTypeHead::UFunc { arg, ret })
    }

    fn obj(&mut self, fields: Vec<(String, Value)>) -> Value {
        let fields = fields.into_iter().collect();
        self.new_val(VTypeHead::VObj { fields })
    }
    fn obj_use(&mut self, field: (String, Use)) -> Use {
        self.new_use(UTypeHead::UObj { field })
    }

    fn case(&mut self, case: (String, Value)) -> Value {
        self.new_val(VTypeHead::VCase { case })
    }
    fn case_use(&mut self, cases: Vec<(String, Use)>) -> Use {
        let cases = cases.into_iter().collect();
        self.new_use(UTypeHead::UCase { cases })
    }

    fn flow(&mut self, lhs: Value, rhs: Use) -> Result<()> {
        let mut pending_edges = vec![(lhs, rhs)];
        let mut type_pairs_to_check = vec![];
        while let Some((lhs, rhs)) = pending_edges.pop() {
            self.r.add_edge(lhs.0, rhs.0, &mut type_pairs_to_check);

            while let Some((lhs, rhs)) = type_pairs_to_check.pop() {
                if let TypeNode::Value(lhs_head) = &self.types[lhs] {
                    if let TypeNode::Use(rhs_head) = &self.types[rhs] {
                        check_heads(lhs_head, rhs_head, &mut pending_edges)?;
                    }
                }
            }
        }
        assert!(pending_edges.is_empty() && type_pairs_to_check.is_empty());
        Ok(())
    }
}

pub struct TypeckState {
    core: TypeCheckerCore,
    bindings: Bindings,
}

impl TypeckState {
    pub fn new() -> Self {
        Self {
            core: TypeCheckerCore::new(),
            bindings: Bindings::new(),
        }
    }

    pub fn check_script(&mut self, parsed: &[ast::TopLevel]) -> Result<()> {
        // Create temporary copy of the entire type state so we can roll
        // back all the changes if the script contains an error.
        let mut temp = self.core.clone();

        for item in parsed {
            if let Err(e) = check_toplevel(&mut self.core, &mut self.bindings, item) {
                // Roll back changes to the type state and bindings
                std::mem::swap(&mut self.core, &mut temp);
                self.bindings.unwind(0);
                return Err(e);
            }
        }

        // Now that script type-checked successfully, make the global definitions permanent
        // by removing them from the changes rollback list
        self.bindings.changes.clear();
        Ok(())
    }
}

fn check_toplevel(
    engine: &mut TypeCheckerCore,
    bindings: &mut Bindings,
    def: &ast::TopLevel,
) -> Result<()> {
    use ast::TopLevel::*;
    match def {
        Expr(expr) => {
            check_expr(engine, bindings, expr)?;
        }
        LetDef((name, var_expr)) => {
            let var_type = check_expr(engine, bindings, var_expr)?;
            bindings.insert(name.clone(), var_type);
        }
        LetRecDef(defs) => {
            let mut temp_bounds = Vec::with_capacity(defs.len());
            for (name, _) in defs {
                let (temp_type, temp_bound) = engine.var();
                bindings.insert(name.clone(), temp_type);
                temp_bounds.push(temp_bound);
            }

            for ((_, expr), bound) in defs.iter().zip(temp_bounds) {
                let var_type = check_expr(engine, bindings, expr)?;
                engine.flow(var_type, bound)?;
            }
        }
    };
    Ok(())
}

fn check_expr(
    engine: &mut TypeCheckerCore,
    bindings: &mut Bindings,
    expr: &ast::Expr,
) -> Result<Value> {
    use ast::Expr::*;
    match expr {
        Literal(val) => {
            use ast::Literal::*;
            match val {
                Bool(_) => Ok(engine.bool()),
            }
        }
        Variable(name) => bindings
            .get(&name)
            .with_context(|| format!("Undefined variable {}", name)),
        Record(fields) => {
            let mut field_names = HashSet::with_capacity(fields.len());
            let mut field_type_pairs = Vec::with_capacity(fields.len());
            for (name, expr) in fields {
                if !field_names.insert(&*name) {
                    bail!("Repeated field name: {}", name);
                }

                let t = check_expr(engine, bindings, expr)?;
                field_type_pairs.push((name.clone(), t));
            }

            Ok(engine.obj(field_type_pairs))
        }
        Case(tag, val_expr) => {
            let val_type = check_expr(engine, bindings, val_expr)?;
            Ok(engine.case((tag.clone(), val_type)))
        }
        If(cond_expr, then_expr, else_expr) => {
            let cond_type = check_expr(engine, bindings, cond_expr)?;
            let bound = engine.bool_use();
            engine.flow(cond_type, bound)?;

            let then_type = check_expr(engine, bindings, then_expr)?;
            let else_type = check_expr(engine, bindings, else_expr)?;

            let (merged, merged_bound) = engine.var();
            engine.flow(then_type, merged_bound)?;
            engine.flow(else_type, merged_bound)?;
            Ok(merged)
        }
        FieldAccess(lhs_expr, name) => {
            let lhs_type = check_expr(engine, bindings, lhs_expr)?;
            let (field_type, field_bound) = engine.var();
            let bound = engine.obj_use((name.clone(), field_bound));
            engine.flow(lhs_type, bound)?;
            Ok(field_type)
        }
        Match(match_expr, cases) => {
            let match_type = check_expr(engine, bindings, match_expr)?;
            let (result_type, result_bound) = engine.var();

            let mut case_names = HashSet::with_capacity(cases.len());
            let mut case_type_pairs = Vec::with_capacity(cases.len());
            for ((tag, name), rhs_expr) in cases {
                if !case_names.insert(&*name) {
                    bail!("Repeated match case {}", name);
                }
                let (wrapped_type, wrapped_bound) = engine.var();
                case_type_pairs.push((tag.clone(), wrapped_bound));

                let rhs_type = bindings.in_child_scope(|bindings| {
                    bindings.insert(name.clone(), wrapped_type);
                    check_expr(engine, bindings, rhs_expr)
                })?;
                engine.flow(rhs_type, result_bound)?;
            }

            let bound = engine.case_use(case_type_pairs);
            engine.flow(match_type, bound)?;
            Ok(result_type)
        }
        FuncDef(arg_name, body_expr) => {
            let (arg_type, arg_bound) = engine.var();
            let body_type = bindings.in_child_scope(|bindings| {
                bindings.insert(arg_name.clone(), arg_type);
                check_expr(engine, bindings, body_expr)
            })?;
            Ok(engine.func(arg_bound, body_type))
        }
        Call(func_expr, arg_expr) => {
            let func_type = check_expr(engine, bindings, func_expr)?;
            let arg_type = check_expr(engine, bindings, arg_expr)?;

            let (ret_type, ret_bound) = engine.var();
            let bound = engine.func_use(arg_type, ret_bound);
            engine.flow(func_type, bound)?;
            Ok(ret_type)
        }
        Let((name, var_expr), rest_expr) => {
            let var_type = check_expr(engine, bindings, var_expr)?;
            bindings.in_child_scope(|bindings| {
                bindings.insert(name.clone(), var_type);
                check_expr(engine, bindings, rest_expr)
            })
        }
        LetRec(defs, rest_expr) => bindings.in_child_scope(|bindings| {
            let mut temp_bounds = Vec::with_capacity(defs.len());
            for (name, _) in defs {
                let (temp_type, temp_bound) = engine.var();
                bindings.insert(name.clone(), temp_type);
                temp_bounds.push(temp_bound);
            }

            for ((_, expr), bound) in defs.iter().zip(temp_bounds) {
                let var_type = check_expr(engine, bindings, expr)?;
                engine.flow(var_type, bound)?;
            }

            check_expr(engine, bindings, rest_expr)
        }),
    }
}
