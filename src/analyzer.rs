use crate::ast::{
    AssignmentTarget, BinaryOperator, ClassField, Expression, FunctionDecl, Literal, Parameter,
    Statement, UnaryOperator,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::io::BufRead; // needed for stdin().lock().read_line()
use std::rc::Rc;


#[derive(Debug, Clone)]
enum Value {
    Int(i64),
    Float(f64),
    Bool(bool),
    Str(String),
    Object(Rc<RefCell<ObjectInstance>>),
    Function(Rc<FunctionValue>),
    BoundMethod {
        receiver: Rc<RefCell<ObjectInstance>>,
        function: Rc<FunctionValue>,
    },
    Class(Rc<ClassValue>),
    Module(Rc<ModuleValue>),
    Map(Rc<RefCell<HashMap<String, Value>>>),
    Array(Rc<RefCell<Vec<Value>>>),
    Null,
}


#[derive(Debug, Clone)]
struct VariableSlot {
    value: Value,
    is_mutable: bool,
    declared_type: Option<String>,
}

#[derive(Debug, Clone)]
struct FunctionValue {
    name: String,
    params: Vec<Parameter>,
    return_type: Option<String>,
    body: Statement,
}

#[derive(Debug, Clone)]
struct ClassValue {
    name: String,
    fields: Vec<ClassField>,
    methods: HashMap<String, Rc<FunctionValue>>,
}

#[derive(Debug, Clone)]
struct ObjectField {
    value: Value,
    is_mutable: bool,
    declared_type: Option<String>,
}

#[derive(Debug, Clone)]
struct ObjectInstance {
    class_name: String,
    fields: HashMap<String, ObjectField>,
    methods: HashMap<String, Rc<FunctionValue>>,
}

#[derive(Debug, Clone)]
struct ModuleValue {
    name: String,
    functions: HashMap<String, Rc<FunctionValue>>,
    classes: HashMap<String, Rc<ClassValue>>,
}


enum ExecOutcome {
    Continue,
    Return(Value),
    Break, 
}


pub struct Analyzer {
    time_cost: usize,
    space_allocations: usize,
    current_depth: usize,
    max_depth: usize,
    scopes: Vec<HashMap<String, VariableSlot>>,
    functions: HashMap<String, Rc<FunctionValue>>,
    classes: HashMap<String, Rc<ClassValue>>,
    modules: HashMap<String, Rc<ModuleValue>>,
}

impl Analyzer {
    pub fn new() -> Self {
        Self {
            time_cost: 0,
            space_allocations: 0,
            current_depth: 0,
            max_depth: 0,
            scopes: vec![HashMap::new()],
            functions: HashMap::new(),
            classes: HashMap::new(),
            modules: HashMap::new(),
        }
    }

    pub fn analyze(&mut self, ast: &[Statement]) {
        println!("\n--- Executing Optimus Script ---");

        for stmt in ast {
            match self.execute_statement(stmt) {
                Ok(ExecOutcome::Continue) => {}
                Ok(ExecOutcome::Return(_)) => {
                    self.print_runtime_error("return can only be used inside a function");
                    break;
                }
                Ok(ExecOutcome::Break) => {
                    self.print_runtime_error("break can only be used inside a loop");
                    break;
                }
                Err(err) => {
                    self.print_runtime_error(&err);
                    break;
                }
            }
        }

        self.print_report();
    }


    fn execute_statement(&mut self, stmt: &Statement) -> Result<ExecOutcome, String> {
        self.time_cost += 1;

        match stmt {
            Statement::VariableDecl { is_mutable, var_type, name, value } => {
                let val = self.evaluate_expression(value)?;
                self.ensure_type_compatibility(var_type, &val)?;
                self.define_variable(name.clone(), val, *is_mutable, Some(var_type.clone()))?;
                Ok(ExecOutcome::Continue)
            }

            Statement::LetDecl { is_mutable, name, value } => {
                let val = self.evaluate_expression(value)?;
                let inferred = Some(Self::type_of_value_static(&val));
                self.define_variable(name.clone(), val, *is_mutable, inferred)?;
                Ok(ExecOutcome::Continue)
            }

            Statement::Assignment { target, value } => {
                let val = self.evaluate_expression(value)?;
                self.assign_target(target, val)?;
                Ok(ExecOutcome::Continue)
            }

            Statement::Print(expr) => {
                let val = self.evaluate_expression(expr)?;
                println!("> {}", self.format_value(&val));
                Ok(ExecOutcome::Continue)
            }

            Statement::Expression(expr) => {
                let _ = self.evaluate_expression(expr)?;
                Ok(ExecOutcome::Continue)
            }

            Statement::Block(stmts) => {
                self.scopes.push(HashMap::new());
                for s in stmts {
                    match self.execute_statement(s)? {
                        ExecOutcome::Continue => {}
                        // Propagate both Return and Break out of blocks
                        outcome => {
                            self.scopes.pop();
                            return Ok(outcome);
                        }
                    }
                }
                self.scopes.pop();
                Ok(ExecOutcome::Continue)
            }

            Statement::If { condition, then_branch, else_branch } => {
                let cond = self.evaluate_expression(condition)?;
                if self.expect_bool(cond, "if condition")? {
                    self.execute_statement(then_branch)
                } else if let Some(else_stmt) = else_branch {
                    self.execute_statement(else_stmt)
                } else {
                    Ok(ExecOutcome::Continue)
                }
            }

            Statement::WhileLoop { condition, body } => {
                self.enter_loop();
                loop {
                    let cond = self.evaluate_expression(condition)?;
                    if !self.expect_bool(cond, "while condition")? {
                        break;
                    }
                    match self.execute_statement(body)? {
                        ExecOutcome::Continue => {}
                        ExecOutcome::Break => break, // NEW
                        outcome @ ExecOutcome::Return(_) => {
                            self.exit_loop();
                            return Ok(outcome);
                        }
                    }
                }
                self.exit_loop();
                Ok(ExecOutcome::Continue)
            }

            Statement::ForLoop { init, condition, increment, body } => {
                self.enter_loop();
                self.scopes.push(HashMap::new());
                self.execute_statement(init)?;

                loop {
                    let cond = self.evaluate_expression(condition)?;
                    if !self.expect_bool(cond, "for condition")? {
                        break;
                    }
                    match self.execute_statement(body)? {
                        ExecOutcome::Continue => {}
                        ExecOutcome::Break => break, // NEW
                        outcome @ ExecOutcome::Return(_) => {
                            self.scopes.pop();
                            self.exit_loop();
                            return Ok(outcome);
                        }
                    }
                    self.execute_statement(increment)?;
                }

                self.scopes.pop();
                self.exit_loop();
                Ok(ExecOutcome::Continue)
            }

          
            Statement::ForIn { var, iterable, body } => {
                let collection = self.evaluate_expression(iterable)?;

                match collection {
                    Value::Array(arr_rc) => {
                        // Snapshot the elements so the body can mutate the array
                        let elements: Vec<Value> = arr_rc.borrow().clone();
                        self.enter_loop();

                        for element in elements {
                            self.scopes.push(HashMap::new());
                            // The loop variable is immutable by default (spec: immutable-by-default)
                            self.define_variable(var.clone(), element, false, None)?;

                            match self.execute_statement(body)? {
                                ExecOutcome::Continue => {}
                                ExecOutcome::Break => {
                                    self.scopes.pop();
                                    break;
                                }
                                outcome @ ExecOutcome::Return(_) => {
                                    self.scopes.pop();
                                    self.exit_loop();
                                    return Ok(outcome);
                                }
                            }
                            self.scopes.pop();
                        }

                        self.exit_loop();
                        Ok(ExecOutcome::Continue)
                    }
                    other => Err(format!(
                        "for-in requires an array, found '{}'",
                        self.type_of_value(&other)
                    )),
                }
            }

          
            Statement::Break => Ok(ExecOutcome::Break),

            Statement::FunctionDecl(decl) => {
                let function = self.function_from_decl(decl);
                self.functions.insert(decl.name.clone(), function);
                Ok(ExecOutcome::Continue)
            }

            Statement::Return(value) => {
                let resolved = match value {
                    Some(expr) => self.evaluate_expression(expr)?,
                    None => Value::Null,
                };
                Ok(ExecOutcome::Return(resolved))
            }

            Statement::ClassDecl { name, fields, methods } => {
                let class = Rc::new(self.class_from_decl(name, fields, methods));
                self.classes.insert(name.clone(), class);
                Ok(ExecOutcome::Continue)
            }

            Statement::ModuleDecl { name, body } => {
                let module = Rc::new(self.module_from_decl(name, body));
                self.modules.insert(name.clone(), module);
                Ok(ExecOutcome::Continue)
            }

            Statement::Import(name) => {
                let module = self
                    .modules
                    .get(name)
                    .cloned()
                    .ok_or_else(|| format!("module '{}' not found", name))?;
                self.define_variable(
                    name.clone(),
                    Value::Module(module),
                    false,
                    Some("module".to_string()),
                )?;
                Ok(ExecOutcome::Continue)
            }
        }
    }


    fn evaluate_expression(&mut self, expr: &Expression) -> Result<Value, String> {
        match expr {
            Expression::Literal(lit) => Ok(self.literal_to_value(lit)),

            Expression::Identifier(name) => self.resolve_identifier(name),

            Expression::UnaryOp { operator, expr } => {
                let value = self.evaluate_expression(expr)?;
                self.evaluate_unary(operator, value)
            }

            Expression::BinaryOp { left, operator, right } => {
                let l = self.evaluate_expression(left)?;
                let r = self.evaluate_expression(right)?;
                self.evaluate_binary(operator, l, r)
            }

            Expression::MemberAccess { object, member } => {
                let obj_val = self.evaluate_expression(object)?;
                self.resolve_member(obj_val, member)
            }

            Expression::Call { callee, arguments } => {
                if let Expression::Identifier(name) = callee.as_ref() {
                    match name.as_str() {
                        "input" | "toInt" | "toFloat" | "toString" => {
                            let mut args = Vec::with_capacity(arguments.len());
                            for arg in arguments {
                                args.push(self.evaluate_expression(arg)?);
                            }
                            return self.call_builtin(name, args);
                        }
                        _ => {}
                    }
                }
                let callee_val = self.evaluate_expression(callee)?;
                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(self.evaluate_expression(arg)?);
                }
                self.call_value(callee_val, args)
            }

            Expression::New { class_name, arguments } => {
                if class_name == "Map" {
                    if !arguments.is_empty() {
                        return Err("Map() takes no constructor arguments".to_string());
                    }
                    self.space_allocations += 1;
                    return Ok(Value::Map(Rc::new(RefCell::new(HashMap::new()))));
                }

                let class = self
                    .classes
                    .get(class_name)
                    .cloned()
                    .ok_or_else(|| format!("class '{}' not found", class_name))?;
                let mut args = Vec::with_capacity(arguments.len());
                for arg in arguments {
                    args.push(self.evaluate_expression(arg)?);
                }
                self.instantiate_class(class, args)
            }

            Expression::Array(elements) => {
                let mut values = Vec::with_capacity(elements.len());
                for el in elements {
                    values.push(self.evaluate_expression(el)?);
                }
                self.space_allocations += values.len();
                Ok(Value::Array(Rc::new(RefCell::new(values))))
            }

            // NEW – index access: arr[i], map["key"]
            Expression::Index { collection, index } => {
                let col = self.evaluate_expression(collection)?;
                let idx = self.evaluate_expression(index)?;
                self.index_get(col, idx)
            }
        }
    }


    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        match name {
            "input" => {
                if !args.is_empty() {
                    return Err("input() takes no arguments".to_string());
                }
                let stdin = std::io::stdin();
                let mut line = String::new();
                stdin
                    .lock()
                    .read_line(&mut line)
                    .map_err(|e| format!("input error: {}", e))?;
                Ok(Value::Str(
                    line.trim_end_matches('\n')
                        .trim_end_matches('\r')
                        .to_string(),
                ))
            }

            "toInt" => {
                if args.len() != 1 {
                    return Err("toInt() takes exactly 1 argument".to_string());
                }
                match &args[0] {
                    Value::Int(v)   => Ok(Value::Int(*v)),
                    Value::Float(v) => Ok(Value::Int(*v as i64)),
                    Value::Bool(b)  => Ok(Value::Int(if *b { 1 } else { 0 })),
                    Value::Str(s)   => s.trim().parse::<i64>().map(Value::Int).map_err(|_| {
                        format!("cannot convert \"{}\" to int", s)
                    }),
                    other => Err(format!(
                        "toInt() cannot convert type '{}'",
                        Self::type_of_value_static(other)
                    )),
                }
            }

            "toFloat" => {
                if args.len() != 1 {
                    return Err("toFloat() takes exactly 1 argument".to_string());
                }
                match &args[0] {
                    Value::Int(v)   => Ok(Value::Float(*v as f64)),
                    Value::Float(v) => Ok(Value::Float(*v)),
                    Value::Bool(b)  => Ok(Value::Float(if *b { 1.0 } else { 0.0 })),
                    Value::Str(s)   => s.trim().parse::<f64>().map(Value::Float).map_err(|_| {
                        format!("cannot convert \"{}\" to float", s)
                    }),
                    other => Err(format!(
                        "toFloat() cannot convert type '{}'",
                        Self::type_of_value_static(other)
                    )),
                }
            }

            "toString" => {
                if args.len() != 1 {
                    return Err("toString() takes exactly 1 argument".to_string());
                }
                Ok(Value::Str(self.format_value(&args[0])))
            }

            other => Err(format!("unknown built-in '{}'", other)),
        }
    }


    fn index_get(&self, collection: Value, index: Value) -> Result<Value, String> {
        match collection {
            Value::Array(arr_rc) => {
                let idx = self.expect_int_index(&index)?;
                let arr = arr_rc.borrow();
                let len = arr.len() as i64;
                if idx < 0 || idx >= len {
                    return Err(format!(
                        "array index {} out of bounds (length {})",
                        idx, len
                    ));
                }
                Ok(arr[idx as usize].clone())
            }
            Value::Map(map_rc) => {
                let key = self.coerce_map_key(&index)?;
                let map = map_rc.borrow();
                map.get(&key)
                    .cloned()
                    .ok_or_else(|| format!("key '{}' not found in map", key))
            }
            other => Err(format!(
                "cannot index into value of type '{}'",
                self.type_of_value(&other)
            )),
        }
    }

    fn index_set(
        &self,
        collection: Value,
        index: Value,
        value: Value,
    ) -> Result<(), String> {
        match collection {
            Value::Array(arr_rc) => {
                let idx = self.expect_int_index(&index)?;
                let mut arr = arr_rc.borrow_mut();
                let len = arr.len() as i64;
                if idx < 0 || idx >= len {
                    return Err(format!(
                        "array index {} out of bounds (length {})",
                        idx, len
                    ));
                }
                arr[idx as usize] = value;
                Ok(())
            }
            Value::Map(map_rc) => {
                let key = self.coerce_map_key(&index)?;
                map_rc.borrow_mut().insert(key, value);
                Ok(())
            }
            other => Err(format!(
                "cannot index into value of type '{}'",
                self.type_of_value(&other)
            )),
        }
    }

    fn expect_int_index(&self, index: &Value) -> Result<i64, String> {
        match index {
            Value::Int(i) => Ok(*i),
            other => Err(format!(
                "array index must be int, found '{}'",
                self.type_of_value(other)
            )),
        }
    }

    fn coerce_map_key(&self, key: &Value) -> Result<String, String> {
        match key {
            Value::Str(s) => Ok(s.clone()),
            Value::Int(i) => Ok(i.to_string()),
            other => Err(format!(
                "map key must be string or int, found '{}'",
                self.type_of_value(other)
            )),
        }
    }

    // ── Identifier / member resolution ─────────────────────────────────────

    fn resolve_identifier(&self, name: &str) -> Result<Value, String> {
        for scope in self.scopes.iter().rev() {
            if let Some(slot) = scope.get(name) {
                return Ok(slot.value.clone());
            }
        }
        if let Some(f) = self.functions.get(name) {
            return Ok(Value::Function(f.clone()));
        }
        if let Some(c) = self.classes.get(name) {
            return Ok(Value::Class(c.clone()));
        }
        if let Some(m) = self.modules.get(name) {
            return Ok(Value::Module(m.clone()));
        }
        Err(format!("undefined identifier '{}'", name))
    }

    fn resolve_member(&self, value: Value, member: &str) -> Result<Value, String> {
        match value {
            Value::Object(obj_ref) => {
                let obj = obj_ref.borrow();
                if let Some(field) = obj.fields.get(member) {
                    return Ok(field.value.clone());
                }
                if let Some(method) = obj.methods.get(member) {
                    return Ok(Value::BoundMethod {
                        receiver: obj_ref.clone(),
                        function: method.clone(),
                    });
                }
                Err(format!(
                    "object of class '{}' has no field or method '{}'",
                    obj.class_name, member
                ))
            }
            Value::Module(module_ref) => {
                if let Some(f) = module_ref.functions.get(member) {
                    return Ok(Value::Function(f.clone()));
                }
                if let Some(c) = module_ref.classes.get(member) {
                    return Ok(Value::Class(c.clone()));
                }
                Err(format!(
                    "module '{}' has no export named '{}'",
                    module_ref.name, member
                ))
            }
            Value::Class(class_ref) => {
                if let Some(method) = class_ref.methods.get(member) {
                    return Ok(Value::Function(method.clone()));
                }
                Err(format!(
                    "class '{}' has no static export named '{}'",
                    class_ref.name, member
                ))
            }
            other => Err(format!(
                "cannot access member '{}' on value of type '{}'",
                member,
                self.type_of_value(&other)
            )),
        }
    }


    fn call_value(&mut self, callee: Value, args: Vec<Value>) -> Result<Value, String> {
        match callee {
            Value::Function(f) => self.call_function(f, args, None),
            Value::BoundMethod { receiver, function } => {
                self.call_function(function, args, Some(receiver))
            }
            Value::Class(class_ref) => self.instantiate_class(class_ref, args),
            other => Err(format!(
                "attempted to call non-callable value of type '{}'",
                self.type_of_value(&other)
            )),
        }
    }

    fn call_function(
        &mut self,
        function: Rc<FunctionValue>,
        args: Vec<Value>,
        receiver: Option<Rc<RefCell<ObjectInstance>>>,
    ) -> Result<Value, String> {
        if args.len() != function.params.len() {
            return Err(format!(
                "function '{}' expected {} arguments but got {}",
                function.name,
                function.params.len(),
                args.len()
            ));
        }

        self.scopes.push(HashMap::new());

        if let Some(obj_ref) = receiver {
            let class_name = obj_ref.borrow().class_name.clone();
            self.define_variable("self".to_string(), Value::Object(obj_ref), false, Some(class_name))?;
        }

        for (param, arg) in function.params.iter().zip(args.into_iter()) {
            self.ensure_type_compatibility(&param.type_name, &arg)?;
            self.define_variable(param.name.clone(), arg, false, Some(param.type_name.clone()))?;
        }

        let execution = self.execute_statement(&function.body);
        self.scopes.pop();

        let return_value = match execution? {
            ExecOutcome::Continue => Value::Null,
            ExecOutcome::Return(v) => v,
            ExecOutcome::Break => {
                return Err("break cannot be used outside a loop".to_string());
            }
        };

        if let Some(expected) = &function.return_type {
            self.ensure_type_compatibility(expected, &return_value)?;
        }

        Ok(return_value)
    }

    fn instantiate_class(
        &mut self,
        class_def: Rc<ClassValue>,
        args: Vec<Value>,
    ) -> Result<Value, String> {
        let mut fields = HashMap::new();

        for field in &class_def.fields {
            let value = if let Some(default_expr) = &field.default_value {
                self.evaluate_expression(default_expr)?
            } else {
                self.default_value_for_type(&field.field_type)
            };
            self.ensure_type_compatibility(&field.field_type, &value)?;
            fields.insert(
                field.name.clone(),
                ObjectField {
                    value,
                    is_mutable: field.is_mutable,
                    declared_type: Some(field.field_type.clone()),
                },
            );
        }

        self.space_allocations += fields.len();

        let object = Rc::new(RefCell::new(ObjectInstance {
            class_name: class_def.name.clone(),
            fields,
            methods: class_def.methods.clone(),
        }));

        if let Some(init) = class_def.methods.get("init") {
            let _ = self.call_function(init.clone(), args, Some(object.clone()))?;
        } else if !args.is_empty() {
            return Err(format!(
                "class '{}' does not define init(), but constructor arguments were provided",
                class_def.name
            ));
        }

        Ok(Value::Object(object))
    }


    fn assign_target(&mut self, target: &AssignmentTarget, value: Value) -> Result<(), String> {
        match target {
            AssignmentTarget::Identifier(name) => self.assign_identifier(name, value),

            AssignmentTarget::MemberAccess { object, member } => {
                let obj_val = self.evaluate_expression(object)?;
                match obj_val {
                    Value::Object(obj_ref) => {
                        let mut obj = obj_ref.borrow_mut();
                        let class_name = obj.class_name.clone();
                        let field = obj.fields.get_mut(member).ok_or_else(|| {
                            format!("class '{}' has no field '{}'", class_name, member)
                        })?;
                        if !field.is_mutable {
                            return Err(format!(
                                "cannot assign to immutable field '{}.{}'",
                                class_name, member
                            ));
                        }
                        if let Some(t) = &field.declared_type {
                            Self::ensure_type_compatibility_static(t, &value)?;
                        }
                        field.value = value;
                        Ok(())
                    }
                    other => Err(format!(
                        "assignment target is not an object field (found '{}')",
                        self.type_of_value(&other)
                    )),
                }
            }

            AssignmentTarget::Index { collection, index } => {
                let col = self.evaluate_expression(collection)?;
                let idx = self.evaluate_expression(index)?;
                self.index_set(col, idx, value)
            }
        }
    }

    fn assign_identifier(&mut self, name: &str, value: Value) -> Result<(), String> {
        for scope in self.scopes.iter_mut().rev() {
            if let Some(slot) = scope.get_mut(name) {
                if !slot.is_mutable {
                    return Err(format!("cannot assign to immutable variable '{}'", name));
                }
                if let Some(t) = &slot.declared_type {
                    Self::ensure_type_compatibility_static(t, &value)?;
                }
                slot.value = value;
                return Ok(());
            }
        }
        Err(format!("cannot assign to undefined variable '{}'", name))
    }

    fn define_variable(
        &mut self,
        name: String,
        value: Value,
        is_mutable: bool,
        declared_type: Option<String>,
    ) -> Result<(), String> {
        let scope = self
            .scopes
            .last_mut()
            .ok_or_else(|| "internal error: no active scope".to_string())?;

        if scope.contains_key(&name) {
            return Err(format!("'{}' is already defined in this scope", name));
        }

        scope.insert(name, VariableSlot { value, is_mutable, declared_type });
        self.space_allocations += 1;
        Ok(())
    }


    fn evaluate_unary(&self, operator: &UnaryOperator, value: Value) -> Result<Value, String> {
        match operator {
            UnaryOperator::Negate => match value {
                Value::Int(v)   => Ok(Value::Int(-v)),
                Value::Float(v) => Ok(Value::Float(-v)),
                other => Err(format!(
                    "cannot apply unary '-' to type '{}'",
                    self.type_of_value(&other)
                )),
            },
            UnaryOperator::Not => match value {
                Value::Bool(v) => Ok(Value::Bool(!v)),
                other => Err(format!(
                    "cannot apply unary '!' to type '{}'",
                    self.type_of_value(&other)
                )),
            },
        }
    }

    fn evaluate_binary(
        &self,
        operator: &BinaryOperator,
        left: Value,
        right: Value,
    ) -> Result<Value, String> {
        match operator {
            BinaryOperator::Add => match (&left, &right) {
                (Value::Int(l),   Value::Int(r))   => Ok(Value::Int(l + r)),
                (Value::Float(l), Value::Float(r)) => Ok(Value::Float(l + r)),
                (Value::Int(l),   Value::Float(r)) => Ok(Value::Float(*l as f64 + r)),
                (Value::Float(l), Value::Int(r))   => Ok(Value::Float(l + *r as f64)),
                (Value::Str(_), _) | (_, Value::Str(_)) => Ok(Value::Str(
                    format!("{}{}", self.format_value(&left), self.format_value(&right)),
                )),
                _ => Err(format!(
                    "operator '+' is not valid for '{}' and '{}'",
                    self.type_of_value(&left),
                    self.type_of_value(&right)
                )),
            },
            BinaryOperator::Subtract => {
                self.numeric_binary(left, right, |l, r| l - r, |l, r| l - r)
            }
            BinaryOperator::Multiply => {
                self.numeric_binary(left, right, |l, r| l * r, |l, r| l * r)
            }
            BinaryOperator::Divide => {
                let zero = matches!(right, Value::Int(0))
                    || matches!(right, Value::Float(v) if v == 0.0);
                if zero {
                    return Err("division by zero".to_string());
                }
                self.numeric_binary(left, right, |l, r| l / r, |l, r| l / r)
            }
            BinaryOperator::Less    => self.numeric_compare(left, right, |l, r| l < r),
            BinaryOperator::Greater => self.numeric_compare(left, right, |l, r| l > r),
            BinaryOperator::Equal   => Ok(Value::Bool(self.values_equal(&left, &right))),
            BinaryOperator::NotEqual => Ok(Value::Bool(!self.values_equal(&left, &right))),
        }
    }

    fn numeric_binary(
        &self,
        left: Value,
        right: Value,
        int_op: impl FnOnce(i64, i64) -> i64,
        float_op: impl FnOnce(f64, f64) -> f64,
    ) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(l),   Value::Int(r))   => Ok(Value::Int(int_op(l, r))),
            (Value::Float(l), Value::Float(r)) => Ok(Value::Float(float_op(l, r))),
            (Value::Int(l),   Value::Float(r)) => Ok(Value::Float(float_op(l as f64, r))),
            (Value::Float(l), Value::Int(r))   => Ok(Value::Float(float_op(l, r as f64))),
            (l, r) => Err(format!(
                "numeric operator requires numbers, found '{}' and '{}'",
                self.type_of_value(&l),
                self.type_of_value(&r)
            )),
        }
    }

    fn numeric_compare(
        &self,
        left: Value,
        right: Value,
        op: impl FnOnce(f64, f64) -> bool,
    ) -> Result<Value, String> {
        match (left, right) {
            (Value::Int(l),   Value::Int(r))   => Ok(Value::Bool(op(l as f64, r as f64))),
            (Value::Float(l), Value::Float(r)) => Ok(Value::Bool(op(l, r))),
            (Value::Int(l),   Value::Float(r)) => Ok(Value::Bool(op(l as f64, r))),
            (Value::Float(l), Value::Int(r))   => Ok(Value::Bool(op(l, r as f64))),
            (l, r) => Err(format!(
                "comparison requires numbers, found '{}' and '{}'",
                self.type_of_value(&l),
                self.type_of_value(&r)
            )),
        }
    }

    fn values_equal(&self, left: &Value, right: &Value) -> bool {
        match (left, right) {
            (Value::Int(l),   Value::Int(r))   => l == r,
            (Value::Float(l), Value::Float(r)) => l == r,
            (Value::Int(l),   Value::Float(r)) => (*l as f64) == *r,
            (Value::Float(l), Value::Int(r))   => *l == (*r as f64),
            (Value::Bool(l),  Value::Bool(r))  => l == r,
            (Value::Str(l),   Value::Str(r))   => l == r,
            (Value::Null,     Value::Null)      => true,
            (Value::Object(l), Value::Object(r)) => Rc::ptr_eq(l, r),
            (Value::Array(l),  Value::Array(r))  => Rc::ptr_eq(l, r),
            (Value::Map(l),    Value::Map(r))    => Rc::ptr_eq(l, r),
            _ => false,
        }
    }


    fn expect_bool(&self, value: Value, context: &str) -> Result<bool, String> {
        match value {
            Value::Bool(v) => Ok(v),
            other => Err(format!(
                "{} must be bool, found '{}'",
                context,
                self.type_of_value(&other)
            )),
        }
    }

    fn ensure_type_compatibility(&self, declared_type: &str, value: &Value) -> Result<(), String> {
        Self::ensure_type_compatibility_static(declared_type, value)
    }

    fn ensure_type_compatibility_static(declared_type: &str, value: &Value) -> Result<(), String> {
        if Self::type_matches_static(declared_type, value) {
            Ok(())
        } else {
            Err(format!(
                "type mismatch: expected '{}', got '{}'",
                declared_type,
                Self::type_of_value_static(value)
            ))
        }
    }

    fn type_matches_static(declared_type: &str, value: &Value) -> bool {
        match declared_type {
            "int"    => matches!(value, Value::Int(_)),
            "float"  => matches!(value, Value::Float(_)),
            "bool"   => matches!(value, Value::Bool(_)),
            "string" => matches!(value, Value::Str(_)),
            "void"   => matches!(value, Value::Null),
            "module" => matches!(value, Value::Module(_)),
            "array"  => matches!(value, Value::Array(_)),  // NEW
            "map"    => matches!(value, Value::Map(_)),    // NEW
            custom => match value {
                Value::Object(obj_ref) => obj_ref.borrow().class_name == custom,
                Value::Null => true,
                _ => false,
            },
        }
    }

    fn type_of_value(&self, value: &Value) -> String {
        Self::type_of_value_static(value)
    }

    fn type_of_value_static(value: &Value) -> String {
        match value {
            Value::Int(_)    => "int".to_string(),
            Value::Float(_)  => "float".to_string(),
            Value::Bool(_)   => "bool".to_string(),
            Value::Str(_)    => "string".to_string(),
            Value::Null      => "null".to_string(),
            Value::Array(_)  => "array".to_string(),  // NEW
            Value::Map(_)    => "map".to_string(),    // NEW
            Value::Object(obj_ref) => obj_ref.borrow().class_name.clone(),
            Value::Function(_) | Value::BoundMethod { .. } => "function".to_string(),
            Value::Class(c)  => format!("class {}", c.name),
            Value::Module(m) => format!("module {}", m.name),
        }
    }

    fn default_value_for_type(&self, type_name: &str) -> Value {
        match type_name {
            "int"    => Value::Int(0),
            "float"  => Value::Float(0.0),
            "bool"   => Value::Bool(false),
            "string" => Value::Str(String::new()),
            _ => Value::Null,
        }
    }

    fn literal_to_value(&self, literal: &Literal) -> Value {
        match literal {
            Literal::Int(v)  => Value::Int(*v),
            Literal::Float(v) => Value::Float(*v),
            Literal::Str(v)  => Value::Str(v.clone()),
            Literal::Bool(v) => Value::Bool(*v),
            Literal::Null    => Value::Null,
        }
    }


    fn format_value(&self, value: &Value) -> String {
        match value {
            Value::Int(v)    => v.to_string(),
            Value::Float(v)  => v.to_string(),
            Value::Bool(v)   => v.to_string(),
            Value::Str(v)    => v.clone(),
            Value::Null      => "none".to_string(), // display as "none" per spec
            Value::Function(f) => format!("<fn {}>", f.name),
            Value::BoundMethod { function, .. } => format!("<method {}>", function.name),
            Value::Class(c)  => format!("<class {}>", c.name),
            Value::Module(m) => format!("<module {}>", m.name),
            Value::Array(arr_rc) => {
                let arr = arr_rc.borrow();
                let items: Vec<String> = arr.iter().map(|v| self.format_value(v)).collect();
                format!("[{}]", items.join(", "))
            }
            Value::Map(map_rc) => {
                let map = map_rc.borrow();
                let mut entries: Vec<String> = map
                    .iter()
                    .map(|(k, v)| format!("{}: {}", k, self.format_value(v)))
                    .collect();
                entries.sort();
                format!("{{{}}}", entries.join(", "))
            }
            Value::Object(obj_ref) => {
                let obj = obj_ref.borrow();
                let mut parts: Vec<String> = obj
                    .fields
                    .iter()
                    .map(|(n, f)| format!("{}: {}", n, self.format_value(&f.value)))
                    .collect();
                parts.sort();
                format!("{} {{ {} }}", obj.class_name, parts.join(", "))
            }
        }
    }


    fn function_from_decl(&self, decl: &FunctionDecl) -> Rc<FunctionValue> {
        Rc::new(FunctionValue {
            name: decl.name.clone(),
            params: decl.params.clone(),
            return_type: decl.return_type.clone(),
            body: decl.body.as_ref().clone(),
        })
    }

    fn class_from_decl(
        &self,
        name: &str,
        fields: &[ClassField],
        methods: &[FunctionDecl],
    ) -> ClassValue {
        let mut method_map = HashMap::new();
        for method in methods {
            method_map.insert(method.name.clone(), self.function_from_decl(method));
        }
        ClassValue {
            name: name.to_string(),
            fields: fields.to_vec(),
            methods: method_map,
        }
    }

    fn module_from_decl(&self, name: &str, body: &[Statement]) -> ModuleValue {
        let mut functions = HashMap::new();
        let mut classes = HashMap::new();
        for stmt in body {
            match stmt {
                Statement::FunctionDecl(decl) => {
                    functions.insert(decl.name.clone(), self.function_from_decl(decl));
                }
                Statement::ClassDecl { name, fields, methods } => {
                    classes.insert(
                        name.clone(),
                        Rc::new(self.class_from_decl(name, fields, methods)),
                    );
                }
                _ => {}
            }
        }
        ModuleValue { name: name.to_string(), functions, classes }
    }

  
    fn enter_loop(&mut self) {
        self.current_depth += 1;
        if self.current_depth > self.max_depth {
            self.max_depth = self.current_depth;
        }
    }

    fn exit_loop(&mut self) {
        if self.current_depth > 0 {
            self.current_depth -= 1;
        }
    }
    
    fn print_runtime_error(&self, error: &str) {
        println!("\nRuntime Error: {}", error);
    }

    fn print_report(&self) {
        println!("\n========================================");
        println!("BIG-O COMPLEXITY REPORT");
        println!("========================================");

        let time = match self.max_depth {
            0 => "O(1)",
            1 => "O(N)",
            2 => "O(N^2)",
            _ => "O(N^X)",
        };

        let space = if self.space_allocations > 100 { "O(N)" } else { "O(1)" };

        println!(
            r#"<div class="complexity-report">
    <span class="metric time">Time Complexity:  {}</span>
    <span class="metric space">Space Complexity: {}</span>
    <span class="metric ops">Operations:       {}</span>
    <span class="metric alloc">Allocations:      {}</span>
    </div>"#,
            time, space, self.time_cost, self.space_allocations
        );
    }
}