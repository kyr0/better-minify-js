use parse_js::{
    ast::{
        ClassOrObjectMemberKey, ClassOrObjectMemberValue, ExportName, ExportNames, NodeId, NodeMap,
        ObjectMemberType, Syntax,
    },
    char::{ID_CONTINUE_CHARSTR, ID_START_CHARSTR},
    lex::KEYWORD_STRS,
    source::SourceRange,
    symbol::{ScopeId, ScopeMap, SymbolMap},
    update::NodeUpdates,
    visit::{JourneyControls, Visitor},
};

const JSX_COMPONENT_NAME_PREFIX: char = '\u{01BC}';
const ALT_MINIFIED_NAMES: &'static [char] = &[
    '\u{01BB}', '\u{02B0}', '\u{02B1}', '\u{02B2}', '\u{02B3}', '\u{02B4}', '\u{02B5}', '\u{02B6}',
    '\u{02B7}', '\u{02B8}', '\u{02B9}', '\u{02BA}', '\u{02BB}', '\u{02BC}', '\u{02BD}', '\u{02BE}',
    '\u{02BF}', '\u{02C0}', '\u{02C1}', '\u{02C6}', '\u{02C7}', '\u{02C8}', '\u{02C9}', '\u{02CA}',
    '\u{02CB}', '\u{02CC}', '\u{02CD}', '\u{02CE}', '\u{02CF}', '\u{02D0}', '\u{02D1}', '\u{02E0}',
    '\u{02E1}', '\u{02E2}', '\u{02E3}', '\u{02E4}', '\u{02EC}', '\u{02EE}', '\u{0374}', '\u{037A}',
    '\u{0559}', '\u{0640}', '\u{06E5}', '\u{06E6}', '\u{07F4}', '\u{07F5}', '\u{07FA}',
];

struct ExportBinding {
    target: SourceRange,
    alias: SourceRange,
}

#[derive(Default)]
struct MinifySymbol {
    // Set to 0 initially, before minification pass. WARNING: 0 is still a valid value, so do not use before setting.
    minified_name_id: usize,
    is_used_as_jsx_component: bool,
}

impl MinifySymbol {
    fn set_minified_name_id(&mut self, id: usize) {
        self.minified_name_id = id;
    }

    fn mark_as_used_as_jsx_component(&mut self) {
        self.is_used_as_jsx_component = true;
    }

    fn generate_minified_name(&self) -> SourceRange {
        let mut id = self.minified_name_id;
        let mut name = Vec::<u8>::new();
        // See main function at bottom for why we do this.
        if self.is_used_as_jsx_component {
            name.extend(b"  ");
            JSX_COMPONENT_NAME_PREFIX.encode_utf8(&mut name[0..2]);
        }
        name.push(ID_START_CHARSTR[id % ID_START_CHARSTR.len()]);
        if id >= ID_START_CHARSTR.len() {
            id /= ID_START_CHARSTR.len();
            while id >= ID_CONTINUE_CHARSTR.len() {
                name.push(ID_CONTINUE_CHARSTR[id % ID_CONTINUE_CHARSTR.len()]);
                id /= ID_CONTINUE_CHARSTR.len();
            }
            name.push(ID_CONTINUE_CHARSTR[id % ID_CONTINUE_CHARSTR.len()]);
        };
        if let Some(alt_id) = KEYWORD_STRS.get(name.as_slice()) {
            // This name is a keyword, so we replace it with a Unicode character instead.
            // This Unicode character is 2 bytes when encoded in UTF-8, so it's more than minimal enough. UTF-8 encodes U+0080 to U+07FF in 2 bytes.
            // There should be exactly one ALT_MINIFIED_NAMES element for each KEYWORD_STRS entry.
            // Using a Unicode name will ensure no chance of clashing with keywords, well-knowns, and almost all variables.
            // Clashes can appear quickly e.g. `in`, `of`, `if`.
            // TODO This could still clash with an untracked or unminified variable; however, that's technically true of any minified name.
            let s = ALT_MINIFIED_NAMES[*alt_id].encode_utf8(&mut name).len();
            name.truncate(s);
        };
        SourceRange::anonymous(name)
    }
}

// See main function at bottom for why we need this.
struct JsxVisitor<'a> {
    nodes: &'a NodeMap,
    scopes: &'a ScopeMap,
    symbols: &'a mut SymbolMap<MinifySymbol>,
}

impl<'a> Visitor for JsxVisitor<'a> {
    fn on_syntax(&mut self, _parent_node: NodeId, node: NodeId, ctl: &mut JourneyControls) -> () {
        let n = &self.nodes[node];
        match n.stx() {
            Syntax::JsxName {
                namespace: None,
                name,
            } if !name.as_slice()[0].is_ascii_lowercase() => {
                match self.scopes[n.scope()].find_symbol(self.scopes, name) {
                    Some(sym) => self.symbols[sym].mark_as_used_as_jsx_component(),
                    None => {
                        // TODO Warn if symbol not found.
                    }
                };
            }
            _ => {}
        }
    }
}

struct MinifyVisitor<'a> {
    // Exports with the same exported name (including multiple default exports) are illegal, so we don't have to worry about/handle that case.
    export_bindings: &'a mut Vec<ExportBinding>,
    nodes: &'a NodeMap,
    scopes: &'a ScopeMap,
    symbols: &'a mut SymbolMap<MinifySymbol>,
    updates: &'a mut NodeUpdates,
}

impl<'a> MinifyVisitor<'a> {
    fn visit_exported_pattern(&mut self, n: NodeId) -> () {
        match self.nodes[n].stx() {
            Syntax::ArrayPattern { elements, rest } => {
                for e in elements {
                    if let Some(e) = e {
                        self.visit_exported_pattern(e.target);
                    }
                }
                if let Some(rest) = rest {
                    self.visit_exported_pattern(*rest);
                }
            }
            Syntax::ObjectPattern { properties, rest } => {
                for p in properties {
                    self.visit_exported_pattern(*p);
                }
                if let Some(rest) = rest {
                    self.visit_exported_pattern(*rest);
                }
            }
            Syntax::ObjectPatternProperty { key, target, .. } => {
                match target {
                    Some(target) => self.visit_exported_pattern(*target),
                    // Shorthand.
                    None => match key {
                        ClassOrObjectMemberKey::Direct(key) => {
                            self.export_bindings.push(ExportBinding {
                                target: key.clone(),
                                alias: key.clone(),
                            })
                        }
                        _ => unreachable!(),
                    },
                }
            }
            Syntax::IdentifierPattern { name } => self.export_bindings.push(ExportBinding {
                target: name.clone(),
                alias: name.clone(),
            }),
            _ => unreachable!(),
        }
    }
}

impl<'a> Visitor for MinifyVisitor<'a> {
    fn on_syntax(&mut self, _parent_node: NodeId, node: NodeId, ctl: &mut JourneyControls) -> () {
        let scope_id = self.nodes[node].scope();
        let scope = &self.scopes[scope_id];
        match self.nodes[node].stx() {
            Syntax::ArrowFunctionExpr {
                parenthesised,
                is_async,
                signature,
                body,
            } => {
                if let Syntax::BlockStmt { body } = self.nodes[*body].stx() {
                    if body.len() == 1 {
                        if let Syntax::ReturnStmt { value } = self.nodes[body[0]].stx() {
                            if let Some(return_value) = value {
                                self.updates.replace_node(
                                    node,
                                    scope_id,
                                    self.nodes[node].loc().clone(),
                                    Syntax::ArrowFunctionExpr {
                                        parenthesised: *parenthesised,
                                        is_async: *is_async,
                                        signature: *signature,
                                        body: *return_value,
                                    },
                                );
                            }
                        };
                    };
                };
            }
            stx @ (Syntax::IdentifierPattern { name }
            | Syntax::IdentifierExpr { name }
            | Syntax::ClassOrFunctionName { name }
            | Syntax::JsxMember { base: name, .. }
            | Syntax::JsxName {
                name,
                namespace: None,
            }) => {
                let sym = scope.find_symbol(self.scopes, name);
                // TODO JsxMember and JsxNamespacedName must be capitalised to be interpreted as a component.
                if let Some(sym) = sym {
                    let minified = self.symbols[sym].generate_minified_name();
                    self.updates.replace_node(
                        node,
                        scope_id,
                        minified.clone(),
                        match stx {
                            Syntax::IdentifierPattern { .. } => Syntax::IdentifierPattern {
                                name: minified.clone(),
                            },
                            Syntax::IdentifierExpr { .. } => Syntax::IdentifierExpr {
                                name: minified.clone(),
                            },
                            Syntax::ClassOrFunctionName { .. } => Syntax::ClassOrFunctionName {
                                name: minified.clone(),
                            },
                            Syntax::JsxMember { path, .. } => Syntax::JsxMember {
                                base: minified.clone(),
                                path: path.clone(),
                            },
                            Syntax::JsxName { namespace, .. } => Syntax::JsxName {
                                namespace: namespace.clone(),
                                name: minified.clone(),
                            },
                            _ => unreachable!(),
                        },
                    );
                };
            }
            Syntax::ExportDeclStmt {
                declaration,
                default,
            } => match self.nodes[*declaration].stx() {
                Syntax::ClassDecl { name, .. } | Syntax::FunctionDecl { name, .. } => {
                    match name {
                        Some(name) => match self.nodes[*name].stx() {
                            Syntax::ClassOrFunctionName { name } => {
                                self.export_bindings.push(ExportBinding {
                                    target: name.clone(),
                                    alias: if *default {
                                        SourceRange::anonymous("default")
                                    } else {
                                        name.clone()
                                    },
                                });
                            }
                            _ => unreachable!(),
                        },
                        _ => {}
                    };
                }
                Syntax::VarStmt { declaration } => match self.nodes[*declaration].stx() {
                    Syntax::VarDecl { declarators, .. } => {
                        for decl in declarators {
                            self.visit_exported_pattern(decl.pattern);
                        }
                    }
                    _ => unreachable!(),
                },
                _ => unreachable!(),
            },
            Syntax::ExportListStmt { names, from } => {
                ctl.skip();
                match from {
                    None => match names {
                        ExportNames::Specific(names) => {
                            for e in names {
                                self.export_bindings.push(ExportBinding {
                                    target: e.target.clone(),
                                    alias: match self.nodes[e.alias].stx() {
                                        Syntax::IdentifierPattern { name } => name.clone(),
                                        _ => unreachable!(),
                                    },
                                });
                                self.updates.replace_node(
                                    node,
                                    scope_id,
                                    SourceRange::anonymous(""),
                                    Syntax::EmptyStmt {},
                                );
                            }
                        }
                        ExportNames::All(_) => unreachable!(),
                    },
                    // `export ... from ...` do not touch/alter the module's scope, so we can ignore completely.
                    _ => {}
                }
            }
            Syntax::ObjectPatternProperty {
                key,
                target,
                default_value,
            } => {
                match key {
                    ClassOrObjectMemberKey::Direct(name) => {
                        if target.is_none() {
                            let sym = scope.find_symbol(self.scopes, name);
                            if let Some(sym) = sym {
                                let minified = self.symbols[sym].generate_minified_name();
                                let replacement_target_node = self.updates.create_node(
                                    scope_id,
                                    minified.clone(),
                                    Syntax::IdentifierPattern {
                                        name: minified.clone(),
                                    },
                                );
                                self.updates.replace_node(
                                    node,
                                    scope_id,
                                    minified.clone(),
                                    Syntax::ObjectPatternProperty {
                                        key: key.clone(),
                                        target: Some(replacement_target_node),
                                        default_value: default_value.clone(),
                                    },
                                );
                            };
                        }
                    }
                    _ => {}
                };
            }
            Syntax::ObjectMember { typ } => {
                match typ {
                    ObjectMemberType::Shorthand { name } => {
                        let sym = scope.find_symbol(self.scopes, name);
                        if let Some(sym) = sym {
                            let minified = self.symbols[sym].generate_minified_name();
                            let replacement_initializer_node = self.updates.create_node(
                                scope_id,
                                minified.clone(),
                                Syntax::IdentifierExpr {
                                    name: minified.clone(),
                                },
                            );
                            self.updates.replace_node(
                                node,
                                scope_id,
                                minified.clone(),
                                Syntax::ObjectMember {
                                    typ: ObjectMemberType::Valued {
                                        key: ClassOrObjectMemberKey::Direct(name.clone()),
                                        value: ClassOrObjectMemberValue::Property {
                                            initializer: Some(replacement_initializer_node),
                                        },
                                    },
                                },
                            );
                        };
                    }
                    _ => {}
                };
            }
            _ => {}
        }
    }
}

pub fn minify_js(
    scope_map: &mut ScopeMap,
    node_map: &mut NodeMap,
    top_level_scope_id: ScopeId,
    top_level_node_id: NodeId,
) -> () {
    let mut symbols: SymbolMap<MinifySymbol> = SymbolMap::<MinifySymbol>::new(&scope_map);
    // We need these since we cannot look up in scope_map while mutably borrowed by iter_mut().
    let symbol_counts: Vec<usize> = scope_map.iter().map(|s| s.symbol_count()).collect();
    let mut minified_name_starts: Vec<usize> = vec![0; scope_map.len()];
    // Iterating like this assumes that any parent of an element is always present before it in the list.
    for (id, scope) in scope_map.iter_mut().enumerate() {
        // TODO Opportunity for optimisation: not all variables are used by descendant scopes, so some of their names can be reused by descendants.
        let minified_name_start = match scope.parent() {
            Some(p) => symbol_counts[p.id()] + minified_name_starts[p.id()],
            None => 0,
        };
        for symbol_id in scope.symbols_iter() {
            symbols[symbol_id]
                .set_minified_name_id(minified_name_start + symbol_id.ordinal_in_scope())
        }
        minified_name_starts[id] = minified_name_start;
    }

    // Since React requires that non-namespaced JSX elements that refer to a JS component must never start with a lowercase letter (otherwise it will always be interpreted as an HTML tag, even if a same-named variable is in scope), we must detect all usages and prefix them with a unique non-lowercase character JSX_COMPONENT_NAME_PREFIX (this is the simplest solution given how we currently minify identifiers). Since we can't know which variables are used as such (for the same reason we cannot minify while parsing in the same step i.e. due to forward references across scopes), we must run a separate pass after parsing and before minifying.
    let mut jsx_visitor = JsxVisitor {
        nodes: node_map,
        scopes: scope_map,
        symbols: &mut symbols,
    };
    jsx_visitor.visit_top_level(node_map, top_level_node_id);

    let mut export_bindings = Vec::new();
    let mut updates = NodeUpdates::new(node_map);
    let mut visitor = MinifyVisitor {
        export_bindings: &mut export_bindings,
        nodes: node_map,
        scopes: scope_map,
        symbols: &mut symbols,
        updates: &mut updates,
    };
    visitor.visit_top_level(node_map, top_level_node_id);
    updates.apply_updates(node_map);
    let export_names = export_bindings
        .iter()
        .map(|e| ExportName {
            target: (symbols[scope_map[top_level_scope_id]
                .find_symbol(scope_map, &e.target)
                .expect(format!("failed to find top-level export `{:?}`", e.target).as_str())]
            .generate_minified_name()),
            alias: node_map.create_node(
                top_level_scope_id,
                e.alias.clone(),
                Syntax::IdentifierPattern {
                    name: e.alias.clone(),
                },
            ),
        })
        .collect::<Vec<ExportName>>();

    if !export_names.is_empty() {
        let final_export_stmt = node_map.create_node(
            top_level_scope_id,
            SourceRange::anonymous(""),
            Syntax::ExportListStmt {
                names: ExportNames::Specific(export_names),
                from: None,
            },
        );
        match node_map[top_level_node_id].stx_mut() {
            Syntax::TopLevel { body } => {
                body.push(final_export_stmt);
            }
            _ => unreachable!(),
        }
    }
}
