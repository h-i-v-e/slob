use sqlparser::dialect::PostgreSqlDialect;
use sqlparser::parser;
use sqlparser::ast::{Statement, Query, SetExpr, SelectItem};
use proc_macro::TokenStream;
use syn::{LitStr, Expr};
use syn::token::Comma;
use syn::parse::{Parse, ParseStream};
use quote::quote;
use regex::Regex;

fn process_query(query: &Query) -> Vec<proc_macro2::TokenStream>{
    let mut out = Vec::<proc_macro2::TokenStream>::new();
    if let SetExpr::Select(select) = &query.body {
        let mut counter: usize = 0;
        for item in &select.projection{
            let token = match item{
                SelectItem::UnnamedExpr(exp) => {
                    exp.to_string()
                },
                SelectItem::ExprWithAlias{
                    expr: _,
                    alias
                } => {
                    alias.to_string()
                },
                _ => "".to_string()
            };
            let alias : proc_macro2::TokenStream = token.parse().unwrap();
            let item = quote! { #alias : row.get(#counter) };
            out.push(item.into());
            counter += 1;
        }
    }
    out
}

struct QueryDef{
    client: Expr,
    sql: LitStr,
    args: Vec<Expr>
}

impl Parse for QueryDef{
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let client : Expr = input.parse()?;
        input.parse::<Comma>()?;
        let sql : LitStr = input.parse()?;
        let mut args = Vec::<Expr>::new();
        while !input.is_empty(){
            input.parse::<Comma>()?;
            args.push(input.parse()?);
        }
        Ok(QueryDef{
           client, sql, args
        })
    }
}

fn escape_interpolation_markers(sql: &LitStr) -> String{
    let reg = Regex::new(r"(\$\d+)").unwrap();
    //sql parsing library is a bit limited and doesn't recognize limit as valid sql
    let offlim = Regex::new(r"(?i)(?:offset|limit)\s+\$?\d+").unwrap();
    let purged = offlim.replace_all(sql.value().as_str(), "").to_string();
    //convert the interpolation markers to strings to trick the sql parser
    //into thinking its valid sql
    reg.replace_all(purged.as_str(), "'$1'").to_string()
}

fn parse_sql(sql: &LitStr) -> Vec<proc_macro2::TokenStream> {
    let dialect = PostgreSqlDialect {};
    let statements: Vec<Statement> = parser::Parser::parse_sql(
        &dialect, escape_interpolation_markers(sql).as_str()
    ).unwrap();
    if let Statement::Query(query) = &statements[0]{
        process_query(query)
    }
    else{
        Vec::new()
    }
}

#[proc_macro]
pub fn one_from(input: TokenStream) -> TokenStream{
    let ast = syn::parse_macro_input!(input as QueryDef);
    let QueryDef{
      client, sql, args
    } = ast;
    let assignments = parse_sql(&sql);
    let out = quote! {
        {
            let res = #client.query_opt(#sql, &[#(#args),*]).await?;
            return Ok(match res{
                Some(row) => Some(Self{#(#assignments),*}),
                None => None
            })
        }
    };
    //println!("{}", out.to_string());
    out.into()
}

#[proc_macro]
pub fn many_from(input: TokenStream) -> TokenStream{
    let ast = syn::parse_macro_input!(input as QueryDef);
    let QueryDef{
        client, sql, args
    } = ast;
    let assignments = parse_sql(&sql);
    let out = quote! {
        {
            let rows = #client.query(#sql, &[#(#args),*]).await?;
            return Ok(rows.iter().map(|row| Self{#(#assignments),*}).collect())
        }
    };
    //println!("{}", out.to_string());
    out.into()
}
