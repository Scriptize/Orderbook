use std::fs::File;
use std::path::Path;
use std::io::{BufReader, BufRead};

fn main() -> std::io::Result<()> {
    if Path::new("schemas.xml").exists() {
        println!("Parsing schemas.xml...");

        let file = File::open("schemas.xml")?;
        let reader = BufReader::new(file);
        let mut generated_code = String::new();


        for line_result in reader.lines() {
            let line = line_result?;

            let tokens: Vec<_> = line.trim_matches(['<', '>'].as_ref())
            .split_whitespace()
            .collect();

            if tokens[0] == "message".as_ref() {
                
            }


            println!("{:?}", tokens);
    
        }
    }
    else{
        println!("schemas.xml is missing");
    }
    Ok(())
}
