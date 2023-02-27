use clap::Parser;
use select::document::Document;
use select::predicate::{Name};
use serde_json::{Value};
use std::fs::File;
use std::io::prelude::*;
use std::thread::sleep;
use std::time::Duration;
use regex::Regex;

pub static COL_SEPARATOR : &'static str = "\t";


async fn livestock_data(progcode: &str) -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all(progcode)?;

    let chart_data_varname = match progcode {
        "livestock" => "chartData",
        _ => "data"
    };

    for state_fips_idx in 1..=56 {
        let fips = state_fips_idx * 1000;
        let resp = reqwest::get(format!("https://farm.ewg.org/progdetail.php?fips={fips:05}&progcode={progcode}"))
            .await;
        if resp.is_err() {
            println!("Error reading URL: {e:?}", e = resp.err());
            continue;
        }
        let resp = resp?;
        let document = Document::from(std::str::from_utf8(resp.bytes().await?.as_ref())?);
        let state = document
            .find(Name("span"))
            .filter(|n| n.attr("class").unwrap_or("").contains("stateface"))
            .flat_map(|n| n.descendants())
            .filter_map(|n| n.as_text())
            .next().expect("Unable to parse state name from HTML");
        if state.contains("United States") {
            continue;
        }

        let mut output_years = File::create(format!("{progcode}/{state}.tsv"))?;

        let year_spending = document
            .find(Name("script"))
            .filter(|n| {
                let chart_data_search = format!("var {chart_data_varname} = ");
                n.html().contains(&chart_data_search)
            })
            .next()
            .map(|n| {
                let html = n.html();
                let chart_data_search = format!("var {chart_data_varname} = ");
                let chart_data_start = html.find(&chart_data_search).unwrap() + chart_data_search.len();
                let chart_data_end = html[chart_data_start..].find(";").unwrap() + chart_data_start;
                let chart_data_str = &html[chart_data_start..chart_data_end];
                // Delete trailing comma
                let chart_data_str = chart_data_str.replace(", ]", "]");
                let chart_data : Value = serde_json::from_str(&chart_data_str).expect("Unable to serialize chartData from string");
                let chart_data_array = chart_data.as_array().expect("Unable to parse chartData as array");
                chart_data_array.iter()
                    .filter_map(|chart_row| chart_row.as_object())
                    .map(|chart_row| {
                        let spending = chart_row.get("spending").expect("Unable to parse chartData row for spending")
                            .as_f64().expect("Unable to parse spending as number");
                        let year = chart_row.get("year").expect("Unable to parse chartData row for year")
                            .as_str().expect("Unable to parse year as string");
                        let mut row_csv = String::new();
                        row_csv.push_str(state);
                        row_csv.push_str(COL_SEPARATOR);
                        row_csv.push_str(&format!("{year}{COL_SEPARATOR}{spending}"));
                        row_csv
                    })
                    .collect::<Vec<String>>()
                    .join("\n")
            })
            .expect("Unable to parse year spending data");
        println!("Year Spending:\n{year_spending}");
        output_years.write_all(vec!["state","year","spending"].join(COL_SEPARATOR).as_bytes())?;
        output_years.write(b"\n")?;
        output_years.write_all(year_spending.as_bytes())?;
    }

    Ok(())
}

use fantoccini::{ClientBuilder};
// Needs a web driver (chromedriver, geckodriver) running on port 9515
async fn spending_data() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("spending")?;
    let states = ["Alabama", "Alaska", "Arizona", "Arkansas", "California", "Colorado", "Connecticut", "Delaware", "District of Columbia", "Florida", "Georgia", "Hawaii", "Idaho", "Illinois", "Indiana", "Iowa", "Kansas", "Kentucky", "Louisiana", "Maine", "Maryland", "Massachusetts", "Michigan", "Minnesota", "Mississippi", "Missouri", "Montana", "Nebraska", "Nevada", "New Hampshire", "New Jersey", "New Mexico", "New York", "North Carolina", "North Dakota", "Ohio", "Oklahoma", "Oregon", "Pennsylvania", "Rhode Island", "South Carolina", "South Dakota", "Tennessee", "Texas", "Utah", "Vermont", "Virginia", "Washington", "West Virginia", "Wisconsin", "Wyoming"];

    let port = 9515;
    let c = ClientBuilder::rustls().connect(&*format!("http://localhost:{port}")).await
        .expect(&*format!("failed to connect to WebDriver on port {port}"));
    for state in states {
        let state_lower = state.to_lowercase();
        let url = format!("https://www.usaspending.gov/state/{state_lower}/latest");

        c.goto(&*url).await?;
        let mut html = c.source().await?;
        while !html.contains("Spending in ") {
            println!("Waiting one second for page to have spending information...");
            sleep(Duration::from_secs(1));
            html = c.source().await?;
        }
        let document = Document::from(html.as_str());
        let mut output_programs = File::create(format!("spending/year_{state}.tsv"))?;

        let re = Regex::new(r"in (\d{4}): (\d+)").unwrap();
        let years = document
            .find(Name("g"))
            .filter(|n| n.attr("aria-label").unwrap_or("").contains("Spending"))
            .map(|n| {
                let spending_str = n.attr("aria-label").unwrap();
                let spending = spending_str
                    .replace(",", "").replace("$", "");
                let captures = re.captures(&spending).unwrap();
                let year = &captures[1];
                let spending = &captures[2];
                let mut row_csv = String::new();
                row_csv.push_str(state);
                row_csv.push_str(COL_SEPARATOR);
                row_csv.push_str(year);
                row_csv.push_str(COL_SEPARATOR);
                row_csv.push_str(spending);
                row_csv
            })
            .collect::<Vec<String>>()
            .join("\n");

        println!("{state}:\n{years}");
        output_programs.write_all(vec!["state","year","spending"].join(COL_SEPARATOR).as_bytes())?;
        output_programs.write(b"\n")?;
        output_programs.write_all(years.as_bytes())?;
    }

    Ok(())
}

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Name of the person to greet
    #[arg(short, long)]
    data: String,
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    match args.data.as_str() {
        "spending" => spending_data().await?,
        s => livestock_data(s).await?
    }

    Ok(())
}