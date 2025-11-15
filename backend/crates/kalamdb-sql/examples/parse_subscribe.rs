use sqlparser::dialect::GenericDialect;
use sqlparser::parser::Parser;

fn main() {
    let sql = "SELECT * FROM app.messages WHERE user_id = CURRENT_USER()";
    let dialect = GenericDialect {};
    match Parser::parse_sql(&dialect, sql) {
        Ok(ast) => println!("Parsed AST: {:?}", ast),
        Err(e) => eprintln!("Parse error: {}", e),
    }
}
