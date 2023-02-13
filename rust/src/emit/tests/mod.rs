use crate::emit::emit_js;
use crate::minify::minify_js;
use crate::TopLevelMode;
use parse_js::lex::Lexer;
use parse_js::parse::Parser;
use parse_js::session::Session;
use std::io::BufWriter;

fn check(top_level_mode: TopLevelMode, src: &str, expected: &str) -> () {
  let session = Session::new();
  let mut parser = Parser::new(Lexer::new(src.as_bytes()));
  let node = parser.parse_top_level(&session, top_level_mode).unwrap();
  let mut out = BufWriter::new(Vec::new());
  minify_js(&session, node);
  emit_js(&mut out, node).unwrap();
  assert_eq!(
    unsafe { std::str::from_utf8_unchecked(out.get_ref().as_slice()) },
    expected
  );
}

#[test]
fn test_emit_global() {
  check(
    TopLevelMode::Global,
    r#"
      /* Test code */
      function * gen () {
        yield * "hello world!";
      }
      !() => {
        com.java.names.long
        module.functions

        function this_is_a_function_decl_not_expr() {
          this_is_a_function_decl_not_expr()
        }

        var the = 1, quick, { brown, _: [ fox, jumped, , , ...over ], ...lazy } = i;

        (( {the} = this_is_a_function_decl_not_expr, [quick] = 2 ) => {
          {
            let brown = this_is_a_function_decl_not_expr(fox);
          }
          the,quick,brown,fox
          ;
          return
          1.2.toString()
        })();;;

        const lorem = ({}) => {}
        const ipsum = (a) => (1,2), dolor = (1/7)/(2/7)
      }()
    "#,
    "\
      function*gen(){yield*\"hello world!\"}\
      !()=>{\
      com.java.names.long;\
      module.functions;\
      function a(){a()}\
      var b=1,c,{brown:d,_:[e,f,,,...g],...h}=i;\
      (({the:l}=a,[m]=2)=>{{let n=a(e)};l,m,d,e;return;1.2.toString()})();\
      const i=({})=>{};\
      const j=l=>(1,2),k=(1/7)/(2/7)\
      }()\
    ",
  )
}

#[test]
fn test_emit_module() {
  check(
    TopLevelMode::Module,
    r#"
      import React, {
        useState as reactUseState,
        useEffect as reactUseEffect,
        createElement,
        memo as reactMemo
      } from "react";
      import {default as ReactDOM} from "react-dom";

      const x = 1;

      export const {meaning} = {meaning: 42}, life = 10;
      console.log("meaning", meaning);

      export default function ship() {};
      console.log(ship(life));

      ReactDOM.hello();

      export {
        reactUseState as use_state,
        reactUseEffect,
      };
    "#,
    "\
      import a,{useState as b,useEffect as c,createElement as d,memo as e}from\"react\";\
      import{default as f}from\"react-dom\";\
      const g=1;\
      const {meaning:h}={meaning:42},i=10;\
      console.log(\"meaning\",h);\
      function j(){};\
      console.log(j(i));\
      f.hello();\
      export{h as meaning,i as life,j as default,b as use_state,c as reactUseEffect}\
    ",
  );
  check(
    TopLevelMode::Module,
    r#"
      export const x = 1;
      export default function(){}
    "#,
    "\
      const a=1;\
      export default function(){};\
      export{a as x}\
    ",
  );
  check(
    TopLevelMode::Module,
    r#"
      export * from "react";
      export default class{}
    "#,
    "\
      export*from\"react\";\
      export default class{}\
    ",
  );
}

#[test]
fn test_emit_private_member() {
  check(
    TopLevelMode::Global,
    r#"
      class A {
        set = 1;
        await
        #hello;
        #goodbye = 1;

        ring() {
          console.log(this.#hello);
        }
      }
    "#,
    "\
      class A{\
      set=1;\
      await;\
      #hello;\
      #goodbye=1;\
      ring(){console.log(this.#hello)}\
      }\
    ",
  );
}

#[test]
fn test_emit_arrow_function_return_expression() {
  check(
    TopLevelMode::Global,
    r#"
      () => {
        return 1;
      }
    "#,
    "()=>1",
  );
  check(
    TopLevelMode::Global,
    r#"
      () => {
        return {};
      }
    "#,
    "()=>({})",
  );
  check(
    TopLevelMode::Global,
    r#"
      () => ({});
    "#,
    "()=>({})",
  );
}

#[test]
fn test_emit_nested_blockless_statements() {
  check(
    TopLevelMode::Global,
    r#"
      function fn(a, b) {
        if (a)
          if (b)
            try {
              c()
            } catch (c) {
              e(f)
            }
          else g = h
      }
    "#,
    "var fn=((a,b)=>{if(a)if(b)try{c()}catch(c){e(f)}else g=h})",
  );
}

#[test]
fn test_emit_jsx() {
  check(
    TopLevelMode::Module,
    r#"
      import CompImp from "./comp";

      let div = {a:"div"};

      const CompLocal = () => <div.a><strong/></div.a>;

      render(<CompImp><CompLocal/></CompImp>);
    "#,
    r#"import Ƽa from"./comp";let b={a:"div"};const Ƽc=()=><b.a><strong/></b.a>;render(<Ƽa><Ƽc/></Ƽa>)"#,
  );
}
