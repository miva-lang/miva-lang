use super::*;

impl Mvm {
    /// Call a builtin function by index.
    pub(super) fn call_builtin(&mut self, idx: u8) -> Result<(), String> {
        // Collect args from stack based on builtin
        match idx {
            // print, prints, println, printlns
            0 => { let v = self.pop(); print!("{}", v.display()); self.push(Value::Unit); } // print
            1 => { let v = self.pop(); match v { Value::String(s) => print!("{}", s), _ => print!("{}", v.display()) }; self.push(Value::Unit); } // prints
            2 => { let v = self.pop(); println!("{}", v.display()); self.push(Value::Unit); } // println
            3 => { let v = self.pop(); match v { Value::String(s) => println!("{}", s), _ => println!("{}", v.display()) }; self.push(Value::Unit); } // printlns
            // error, errors, errorln, errorlns
            4 => { let v = self.pop(); eprint!("{}", v.display()); self.push(Value::Unit); }
            5 => { let v = self.pop(); match v { Value::String(s) => eprint!("{}", s), _ => eprint!("{}", v.display()) }; self.push(Value::Unit); }
            6 => { let v = self.pop(); eprintln!("{}", v.display()); self.push(Value::Unit); }
            7 => { let v = self.pop(); match v { Value::String(s) => eprintln!("{}", s), _ => eprintln!("{}", v.display()) }; self.push(Value::Unit); }
            // exit
            8 => { self.exit_code = self.pop().as_i64().unwrap_or(0); self.halted = true; return Ok(()); }
            // abort
            9 => { eprintln!("MVM: abort called"); std::process::exit(1); }
            // panic
            10 => {
                let msg = match self.pop() { Value::String(s) => (*s).clone(), v => v.display() };
                eprintln!("MVM panic: {}", msg);
                std::process::exit(1);
            }
            // string_concat
            11 => {
                let b = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("string_concat expected string, got {}", v.type_name())) };
                let a = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("string_concat expected string, got {}", v.type_name())) };
                self.push(Value::String(Arc::new(a + &b)));
            }
            // string_length
            12 => {
                match self.pop() { Value::String(s) => self.push(Value::Int(s.len() as i64)), v => return Err(format!("string_length expected string, got {}", v.type_name())) };
            }
            // string_parse
            13 => {
                match self.pop() { Value::String(s) => { let n = s.trim().parse().unwrap_or(0); self.push(Value::Int(n)); } v => return Err(format!("string_parse expected string, got {}", v.type_name())) };
            }
            // string_make
            14 => {
                let len = self.pop().as_i64().ok_or("string_make expected int")? as usize;
                match self.pop() { Value::Char(c) => { let s: String = std::iter::repeat(c as char).take(len).collect(); self.push(Value::String(Arc::new(s))); } v => return Err(format!("string_make expected char, got {}", v.type_name())) };
            }
            // string_from (to_string)
            15 => {
                let v = self.pop();
                self.push(Value::String(Arc::new(v.display())));
            }
            // string_get
            16 => {
                let idx = self.pop().as_i64().ok_or("string_get expected int")? as usize;
                match self.pop() { Value::String(s) => { let c = s.chars().nth(idx).unwrap_or('\0'); self.push(Value::Char(c as u8)); } v => return Err(format!("string_get expected string, got {}", v.type_name())) };
            }
            // box_new
            17 => { let v = self.pop(); self.push(Value::Boxed(Arc::new(Mutex::new(v)))); }
            // box_deref
            18 => { match self.pop() { Value::Boxed(b) => self.push(b.lock().unwrap().clone()), v => return Err(format!("box_deref expected box, got {}", v.type_name())) }; }
            // box_set
            19 => { let val = self.pop(); match self.pop() { Value::Boxed(b) => { *b.lock().unwrap() = val; self.push(Value::Unit); } v => return Err(format!("box_set expected box, got {}", v.type_name())) }; }
            // range
            20 => {
                let end = self.pop().as_i64().ok_or("range expected int end")?;
                // Handle both range(n) and range(start, end)
                // If there's another value on the stack, it's start
                let start = if self.stack.len() > 0 && matches!(self.stack.last(), Some(Value::Int(_))) {
                    self.pop().as_i64().unwrap()
                } else {
                    0i64
                };
                self.push(Value::Range(start, end, start));
            }
            // to_string (same as string_from)
            21 => {
                let v = self.pop();
                self.push(Value::String(Arc::new(v.display())));
            }
            // read_int
            22 => {
                let mut line = String::new();
                io::stdin().lock().read_line(&mut line).ok();
                let n = line.trim().parse::<i64>().unwrap_or(0);
                self.push(Value::Int(n));
            }
            // read_line
            23 => {
                let mut line = String::new();
                io::stdin().lock().read_line(&mut line).ok();
                if line.ends_with('\n') { line.pop(); }
                self.push(Value::String(Arc::new(line)));
            }
            // json_parse
            24 => {
                let s = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("json_parse expected string, got {}", v.type_name())) };
                let val = serde_json::from_str(&s).map_err(|e| format!("JSON parse error: {}", e))?;
                self.push(Value::Json(Box::new(val)));
            }
            // json_kind
            25 => {
                let v = self.pop();
                let kind = match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Null => 0,
                        JsonValue::Bool(_) => 1,
                        JsonValue::Number(_) => 2,
                        JsonValue::String(_) => 3,
                        JsonValue::Array(_) => 4,
                        JsonValue::Object(_) => 5,
                    },
                    _ => -1,
                };
                self.push(Value::Int(kind));
            }
            // json_bool
            26 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Bool(b) => self.push(Value::Bool(*b)),
                        _ => return Err("json_bool: value is not a bool".into()),
                    },
                    _ => return Err("json_bool: expected json value".into()),
                }
            }
            // json_number
            27 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Number(n) => {
                            let f = n.as_f64().unwrap_or(0.0);
                            self.push(Value::Float64(f));
                        }
                        _ => return Err("json_number: value is not a number".into()),
                    },
                    _ => return Err("json_number: expected json value".into()),
                }
            }
            // json_string
            28 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::String(s) => self.push(Value::String(Arc::new(s.clone()))),
                        _ => return Err("json_string: value is not a string".into()),
                    },
                    _ => return Err("json_string: expected json value".into()),
                }
            }
            // json_array_len
            29 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => self.push(Value::Int(a.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("json_array_len: expected json value".into()),
                }
            }
            // json_array_get
            30 => {
                let idx = self.pop().as_i64().ok_or("json_array_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => {
                            if idx >= a.len() {
                                return Err(format!("json_array_get: index {} out of bounds (len={})", idx, a.len()));
                            }
                            self.push(Value::Json(Box::new(a[idx].clone())));
                        }
                        _ => return Err("json_array_get: value is not an array".into()),
                    },
                    _ => return Err("json_array_get: expected json value".into()),
                }
            }
            // json_object_len
            31 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => self.push(Value::Int(o.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("json_object_len: expected json value".into()),
                }
            }
            // json_object_key
            32 => {
                let idx = self.pop().as_i64().ok_or("json_object_key expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("json_object_key: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let key = o.keys().nth(idx).unwrap().clone();
                            self.push(Value::String(Arc::new(key)));
                        }
                        _ => return Err("json_object_key: value is not an object".into()),
                    },
                    _ => return Err("json_object_key: expected json value".into()),
                }
            }
            // json_object_get
            33 => {
                let idx = self.pop().as_i64().ok_or("json_object_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("json_object_get: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let val = o.values().nth(idx).unwrap().clone();
                            self.push(Value::Json(Box::new(val)));
                        }
                        _ => return Err("json_object_get: value is not an object".into()),
                    },
                    _ => return Err("json_object_get: expected json value".into()),
                }
            }
            // json_object_find
            34 => {
                let key = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("json_object_find expected string key, got {}", v.type_name())) };
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if let Some(val) = o.get(&key) {
                                self.push(Value::Json(Box::new(val.clone())));
                            } else {
                                self.push(Value::Json(Box::new(JsonValue::Null)));
                            }
                        }
                        _ => return Err("json_object_find: value is not an object".into()),
                    },
                    _ => return Err("json_object_find: expected json value".into()),
                }
            }
            // json_free
            35 => {
                let _v = self.pop();
                // json_free is a no-op in the MVM because Value::Json owns its data
                self.push(Value::Unit);
            }
            // json_stringify
            36 => {
                let v = self.pop();
                let s = match &v {
                    Value::Json(j) => j.to_string(),
                    _ => return Err("json_stringify: expected json value".into()),
                };
                self.push(Value::String(Arc::new(s)));
            }
            // xml_parse
            37 => {
                let s = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("xml_parse expected string, got {}", v.type_name())) };
                match crate::xml::parse(&s) {
                    Ok(node) => self.push(Value::Xml(node)),
                    Err(e) => return Err(format!("XML parse error: {}", e)),
                }
            }
            // xml_kind
            38 => {
                let kind = match self.pop() {
                    Value::Xml(n) => n.kind.as_u8() as i64,
                    v => return Err(format!("xml_kind expected xml value, got {}", v.type_name())),
                };
                self.push(Value::Int(kind));
            }
            // xml_tag
            39 => {
                let tag = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element { return Err("xml_tag: value is not an element".into()); }
                        n.tag.clone()
                    }
                    v => return Err(format!("xml_tag expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(tag)));
            }
            // xml_attr_count
            40 => {
                let count = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element { 0 } else { n.attrs.len() as i64 }
                    }
                    v => return Err(format!("xml_attr_count expected xml value, got {}", v.type_name())),
                };
                self.push(Value::Int(count));
            }
            // xml_attr_name
            41 => {
                let idx = self.pop().as_i64().ok_or("xml_attr_name expected int")? as usize;
                let name = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element { return Err("xml_attr_name: value is not an element".into()); }
                        if idx >= n.attrs.len() { return Err(format!("xml_attr_name: index {} out of bounds (len={})", idx, n.attrs.len())); }
                        n.attrs[idx].0.clone()
                    }
                    v => return Err(format!("xml_attr_name expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(name)));
            }
            // xml_attr_value
            42 => {
                let idx = self.pop().as_i64().ok_or("xml_attr_value expected int")? as usize;
                let val = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element { return Err("xml_attr_value: value is not an element".into()); }
                        if idx >= n.attrs.len() { return Err(format!("xml_attr_value: index {} out of bounds (len={})", idx, n.attrs.len())); }
                        n.attrs[idx].1.clone()
                    }
                    v => return Err(format!("xml_attr_value expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(val)));
            }
            // xml_attr_find
            43 => {
                let name = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("xml_attr_find expected string, got {}", v.type_name())) };
                let val = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element { String::new() }
                        else { n.attrs.iter().find(|(k, _)| k == &name).map(|(_, v)| v.clone()).unwrap_or_default() }
                    }
                    v => return Err(format!("xml_attr_find expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(val)));
            }
            // xml_child_count
            44 => {
                let count = match self.pop() {
                    Value::Xml(n) => {
                        if n.kind == crate::xml::XmlKind::Element || n.kind == crate::xml::XmlKind::Document { n.children.len() as i64 } else { 0 }
                    }
                    v => return Err(format!("xml_child_count expected xml value, got {}", v.type_name())),
                };
                self.push(Value::Int(count));
            }
            // xml_child_get
            45 => {
                let idx = self.pop().as_i64().ok_or("xml_child_get expected int")? as usize;
                match self.pop() {
                    Value::Xml(n) => {
                        if n.kind != crate::xml::XmlKind::Element && n.kind != crate::xml::XmlKind::Document {
                            return Err("xml_child_get: value is not an element or document".into());
                        }
                        if idx >= n.children.len() { return Err(format!("xml_child_get: index {} out of bounds (len={})", idx, n.children.len())); }
                        self.push(Value::Xml(n.children[idx].clone()));
                    }
                    v => return Err(format!("xml_child_get expected xml value, got {}", v.type_name())),
                }
            }
            // xml_text
            46 => {
                let text = match self.pop() {
                    Value::Xml(n) => n.text.clone(),
                    v => return Err(format!("xml_text expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(text)));
            }
            // xml_comment
            47 => {
                let text = match self.pop() {
                    Value::Xml(n) => n.text.clone(),
                    v => return Err(format!("xml_comment expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(text)));
            }
            // xml_cdata
            48 => {
                let text = match self.pop() {
                    Value::Xml(n) => n.text.clone(),
                    v => return Err(format!("xml_cdata expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(text)));
            }
            // xml_pi_target
            49 => {
                let t = match self.pop() {
                    Value::Xml(n) => n.pi_target.clone(),
                    v => return Err(format!("xml_pi_target expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(t)));
            }
            // xml_pi_data
            50 => {
                let d = match self.pop() {
                    Value::Xml(n) => n.pi_data.clone(),
                    v => return Err(format!("xml_pi_data expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(d)));
            }
            // xml_stringify
            51 => {
                let s = match self.pop() {
                    Value::Xml(n) => crate::xml::stringify(&n),
                    v => return Err(format!("xml_stringify expected xml value, got {}", v.type_name())),
                };
                self.push(Value::String(Arc::new(s)));
            }
            // xml_free
            52 => {
                let _v = self.pop();
                // xml_free is a no-op in the MVM because Value::Xml owns its data
                self.push(Value::Unit);
            }
            // toml_parse
            53 => {
                let s = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("toml_parse expected string, got {}", v.type_name())) };
                match crate::toml::parse(&s) {
                    Ok(val) => self.push(Value::Json(Box::new(val))),
                    Err(e) => return Err(e),
                }
            }
            54 => {
                let v = self.pop();
                let kind = match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Null => 0,
                        JsonValue::Bool(_) => 1,
                        JsonValue::Number(_) => 2,
                        JsonValue::String(_) => 3,
                        JsonValue::Array(_) => 4,
                        JsonValue::Object(_) => 5,
                    },
                    _ => -1,
                };
                self.push(Value::Int(kind));
            }
            // toml_bool
            55 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Bool(b) => self.push(Value::Bool(*b)),
                        _ => return Err("toml_bool: value is not a bool".into()),
                    },
                    _ => return Err("toml_bool: expected toml value".into()),
                }
            }
            // toml_number
            56 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Number(n) => {
                            let f = n.as_f64().unwrap_or(0.0);
                            self.push(Value::Float64(f));
                        }
                        _ => return Err("toml_number: value is not a number".into()),
                    },
                    _ => return Err("toml_number: expected toml value".into()),
                }
            }
            // toml_string
            57 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::String(s) => self.push(Value::String(Arc::new(s.clone()))),
                        _ => return Err("toml_string: value is not a string".into()),
                    },
                    _ => return Err("toml_string: expected toml value".into()),
                }
            }
            // toml_array_len
            58 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => self.push(Value::Int(a.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("toml_array_len: expected toml value".into()),
                }
            }
            // toml_array_get
            59 => {
                let idx = self.pop().as_i64().ok_or("toml_array_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => {
                            if idx >= a.len() {
                                return Err(format!("toml_array_get: index {} out of bounds (len={})", idx, a.len()));
                            }
                            self.push(Value::Json(Box::new(a[idx].clone())));
                        }
                        _ => return Err("toml_array_get: value is not an array".into()),
                    },
                    _ => return Err("toml_array_get: expected toml value".into()),
                }
            }
            // toml_object_len
            60 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => self.push(Value::Int(o.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("toml_object_len: expected toml value".into()),
                }
            }
            // toml_object_key
            61 => {
                let idx = self.pop().as_i64().ok_or("toml_object_key expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("toml_object_key: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let key = o.keys().nth(idx).unwrap().clone();
                            self.push(Value::String(Arc::new(key)));
                        }
                        _ => return Err("toml_object_key: value is not an object".into()),
                    },
                    _ => return Err("toml_object_key: expected toml value".into()),
                }
            }
            // toml_object_get
            62 => {
                let idx = self.pop().as_i64().ok_or("toml_object_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("toml_object_get: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let val = o.values().nth(idx).unwrap().clone();
                            self.push(Value::Json(Box::new(val)));
                        }
                        _ => return Err("toml_object_get: value is not an object".into()),
                    },
                    _ => return Err("toml_object_get: expected toml value".into()),
                }
            }
            // toml_object_find
            63 => {
                let key = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("toml_object_find expected string key, got {}", v.type_name())) };
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if let Some(val) = o.get(&key) {
                                self.push(Value::Json(Box::new(val.clone())));
                            } else {
                                self.push(Value::Json(Box::new(JsonValue::Null)));
                            }
                        }
                        _ => return Err("toml_object_find: value is not an object".into()),
                    },
                    _ => return Err("toml_object_find: expected toml value".into()),
                }
            }
            // toml_free
            64 => {
                let _v = self.pop();
                // toml_free is a no-op in the MVM because Value::Json owns its data
                self.push(Value::Unit);
            }
            // toml_stringify
            65 => {
                let v = self.pop();
                let s = match &v {
                    Value::Json(j) => crate::toml::stringify(j),
                    _ => return Err("toml_stringify: expected toml value".into()),
                };
                self.push(Value::String(Arc::new(s)));
            }
            // yaml_parse
            66 => {
                let s = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("yaml_parse expected string, got {}", v.type_name())) };
                match crate::yaml::parse(&s) {
                    Ok(val) => self.push(Value::Json(Box::new(val))),
                    Err(e) => return Err(e),
                }
            }
            // yaml_kind
            67 => {
                let v = self.pop();
                let kind = match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Null => 0,
                        JsonValue::Bool(_) => 1,
                        JsonValue::Number(_) => 2,
                        JsonValue::String(_) => 3,
                        JsonValue::Array(_) => 4,
                        JsonValue::Object(_) => 5,
                    },
                    _ => -1,
                };
                self.push(Value::Int(kind));
            }
            // yaml_bool
            68 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Bool(b) => self.push(Value::Bool(*b)),
                        _ => return Err("yaml_bool: value is not a bool".into()),
                    },
                    _ => return Err("yaml_bool: expected yaml value".into()),
                }
            }
            // yaml_number
            69 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Number(n) => {
                            let f = n.as_f64().unwrap_or(0.0);
                            self.push(Value::Float64(f));
                        }
                        _ => return Err("yaml_number: value is not a number".into()),
                    },
                    _ => return Err("yaml_number: expected yaml value".into()),
                }
            }
            // yaml_string
            70 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::String(s) => self.push(Value::String(Arc::new(s.clone()))),
                        _ => return Err("yaml_string: value is not a string".into()),
                    },
                    _ => return Err("yaml_string: expected yaml value".into()),
                }
            }
            // yaml_array_len
            71 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => self.push(Value::Int(a.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("yaml_array_len: expected yaml value".into()),
                }
            }
            // yaml_array_get
            72 => {
                let idx = self.pop().as_i64().ok_or("yaml_array_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Array(a) => {
                            if idx >= a.len() {
                                return Err(format!("yaml_array_get: index {} out of bounds (len={})", idx, a.len()));
                            }
                            self.push(Value::Json(Box::new(a[idx].clone())));
                        }
                        _ => return Err("yaml_array_get: value is not an array".into()),
                    },
                    _ => return Err("yaml_array_get: expected yaml value".into()),
                }
            }
            // yaml_object_len
            73 => {
                let v = self.pop();
                match &v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => self.push(Value::Int(o.len() as i64)),
                        _ => self.push(Value::Int(0)),
                    },
                    _ => return Err("yaml_object_len: expected yaml value".into()),
                }
            }
            // yaml_object_key
            74 => {
                let idx = self.pop().as_i64().ok_or("yaml_object_key expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("yaml_object_key: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let key = o.keys().nth(idx).unwrap().clone();
                            self.push(Value::String(Arc::new(key)));
                        }
                        _ => return Err("yaml_object_key: value is not an object".into()),
                    },
                    _ => return Err("yaml_object_key: expected yaml value".into()),
                }
            }
            // yaml_object_get
            75 => {
                let idx = self.pop().as_i64().ok_or("yaml_object_get expected int")? as usize;
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if idx >= o.len() {
                                return Err(format!("yaml_object_get: index {} out of bounds (len={})", idx, o.len()));
                            }
                            let val = o.values().nth(idx).unwrap().clone();
                            self.push(Value::Json(Box::new(val)));
                        }
                        _ => return Err("yaml_object_get: value is not an object".into()),
                    },
                    _ => return Err("yaml_object_get: expected yaml value".into()),
                }
            }
            // yaml_object_find
            76 => {
                let key = match self.pop() { Value::String(s) => (*s).clone(), v => return Err(format!("yaml_object_find expected string key, got {}", v.type_name())) };
                let v = self.pop();
                match v {
                    Value::Json(j) => match j.as_ref() {
                        JsonValue::Object(o) => {
                            if let Some(val) = o.get(&key) {
                                self.push(Value::Json(Box::new(val.clone())));
                            } else {
                                self.push(Value::Json(Box::new(JsonValue::Null)));
                            }
                        }
                        _ => return Err("yaml_object_find: value is not an object".into()),
                    },
                    _ => return Err("yaml_object_find: expected yaml value".into()),
                }
            }
            // yaml_free
            77 => {
                let _v = self.pop();
                // yaml_free is a no-op in the MVM because Value::Json owns its data
                self.push(Value::Unit);
            }
            // yaml_stringify
            78 => {
                let v = self.pop();
                let s = match &v {
                    Value::Json(j) => crate::yaml::stringify(j),
                    _ => return Err("yaml_stringify: expected yaml value".into()),
                };
                self.push(Value::String(Arc::new(s)));
            }
            // ptr_alloc(n_bytes) -> allocate n_bytes/8 Value slots, return start index
            // Since elem_size[T]() = 8 for all T (stdlib), byte counts are always
            // multiples of 8. We divide by 8 to get the number of Value slots.
            79 => {
                let bytes = self.pop().as_i64().ok_or("ptr_alloc expected int")? as usize;
                let n = bytes / 8;
                let base = self.memory.len();
                self.memory.resize(base + n, Value::Int(0));
                self.push(Value::Int(base as i64));
            }
            // ptr_free(p) -> free (no-op in flat memory)
            80 => {
                let _p = self.pop().as_i64().ok_or("ptr_free expected int")?;
                self.push(Value::Unit);
            }
            // ptr_realloc(p, n_bytes) -> reallocate to n_bytes/8 Value slots
            81 => {
                let bytes = self.pop().as_i64().ok_or("ptr_realloc expected int")? as usize;
                let p = self.pop().as_i64().ok_or("ptr_realloc expected int")? as usize;
                let n = bytes / 8 + (if bytes % 8 > 0 { 1 } else { 0 });
                let old_size = if p < self.memory.len() {
                    self.memory.len() - p
                } else { 0 };
                let base = self.memory.len();
                self.memory.resize(base + n, Value::Int(0));
                let copy_len = old_size.min(n);
                for i in 0..copy_len {
                    self.memory[base + i] = std::mem::replace(&mut self.memory[p + i], Value::Int(0));
                }
                self.push(Value::Int(base as i64));
            }
            // ptr_offset(p, n_bytes) -> p + n_bytes/8 (slot index)
            82 => {
                let n_val = self.pop();
                let p_val = self.pop();
                let n = n_val.as_i64().ok_or("ptr_offset expected int")? as i64;
                let p = p_val.as_i64().ok_or("ptr_offset expected int")?;
                let slot_offset = n / 8;
                self.push(Value::Int(p + slot_offset));
            }
            // ptr_set(p, val) -> write val at memory[p]
            83 => {
                let val = self.pop();
                let p = self.pop().as_i64().ok_or("ptr_set expected int")? as usize;
                if p >= self.memory.len() {
                    self.memory.resize(p + 1, Value::Unit);
                }
                self.memory[p] = val;
                self.push(Value::Unit);
            }
            // ptr_ref(p) -> read from memory[p]
            84 => {
                let p = self.pop().as_i64().ok_or("ptr_ref expected int")? as usize;
                if p >= self.memory.len() {
                    return Err("ptr_ref: out of bounds".into());
                }
                self.push(self.memory[p].clone());
            }
            // mutex_new -> create Mutex<()> on heap, store (raw, null) in table
            85 => {
                let id = self.mutex_next_id;
                self.mutex_next_id += 1;
                let boxed = Box::new(std::sync::Mutex::new(()));
                let raw = Box::into_raw(boxed) as *const std::sync::Mutex<()>;
                self.mutex_table.insert(id, (raw, std::ptr::null()));
                self.push(Value::Int(id));
            }
            // mutex_lock -> pop handle, block until lock acquired, store guard ptr
            86 => {
                let handle = self.pop().as_i64().ok_or("mutex_lock expected int handle")?;
                match self.mutex_table.get_mut(&handle) {
                    Some((raw, guard_ptr)) => {
                        if !guard_ptr.is_null() {
                            return Err(format!("mutex_lock: handle {} already locked (non-reentrant)", handle));
                        }
                        let guard = unsafe { raw.as_ref().unwrap().lock().unwrap_or_else(|e| e.into_inner()) };
                        let leaked = unsafe { std::mem::transmute::<std::sync::MutexGuard<'_, ()>, std::sync::MutexGuard<'static, ()>>(guard) };
                        *guard_ptr = Box::leak(Box::new(leaked));
                    }
                    None => return Err(format!("mutex_lock: invalid handle {}", handle)),
                }
                self.push(Value::Unit);
            }
            // mutex_unlock -> pop handle, drop guard (set ptr to null)
            87 => {
                let handle = self.pop().as_i64().ok_or("mutex_unlock expected int handle")?;
                match self.mutex_table.get_mut(&handle) {
                    Some((_raw, guard_ptr)) => {
                        if guard_ptr.is_null() {
                            return Err(format!("mutex_unlock: handle {} is not locked", handle));
                        }
                        unsafe { drop(Box::from_raw(*guard_ptr as *mut MutexGuard<'static, ()>)) };
                        *guard_ptr = std::ptr::null();
                    }
                    None => return Err(format!("mutex_unlock: invalid handle {}", handle)),
                }
                self.push(Value::Unit);
            }
            // mutex_free -> pop handle, drop guard (if any) and free mutex memory
            88 => {
                let handle = self.pop().as_i64().ok_or("mutex_free expected int handle")?;
                if let Some((raw, guard_ptr)) = self.mutex_table.remove(&handle) {
                    if !guard_ptr.is_null() {
                        unsafe { drop(Box::from_raw(guard_ptr as *mut MutexGuard<'static, ()>)) };
                    }
                    unsafe { drop(Box::from_raw(raw as *mut std::sync::Mutex<()>)) };
                }
                self.push(Value::Unit);
            }
            _ => return Err(format!("Unknown builtin index: {}", idx)),
        }
        // Ensure stdout/stderr are flushed
        io::stdout().flush().ok();
        io::stderr().flush().ok();
        Ok(())
    }
}
