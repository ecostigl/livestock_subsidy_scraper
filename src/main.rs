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

async fn livestock_data() -> Result<(), Box<dyn std::error::Error>> {
    std::fs::create_dir_all("programs")?;
    std::fs::create_dir_all("year")?;

    for state_fips_idx in 1..=56 {
        let fips = state_fips_idx * 1000;
        let resp = reqwest::get(format!("https://farm.ewg.org/progdetail.php?fips={fips:05}&progcode=livestock"))
            .await;
        if resp.is_err() {
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

        let mut output_programs = File::create(format!("programs/programs_{state}.tsv"))?;
        let mut output_years = File::create(format!("year/year_{state}.tsv"))?;

        let table = document
            .find(Name("table"))
            .filter(|n| n.attr("title").unwrap_or("") == "Programs included in livestock subsidies")
            .next();

        let table = if table.is_none() {
            continue;
        } else {
            table.unwrap()
        };

        let programs = table
            .find(Name("tr"))
            .skip(1)
            .map(|row| row.descendants()
                .filter_map(|n| n.as_text())
                .map(|txt| txt.replace(",", "").replace("$", ""))
                .collect::<Vec<String>>()
                .join(COL_SEPARATOR))
            .map(|row_values| {
                let mut row_csv = String::new();
                row_csv.push_str(state);
                row_csv.push_str(COL_SEPARATOR);
                row_csv.push_str(&row_values);
                row_csv
            })
            .collect::<Vec<String>>()
            .join("\n");
        println!("Programs:\n{programs}");
        output_programs.write_all(vec!["state","program","spending"].join(COL_SEPARATOR).as_bytes())?;
        output_programs.write(b"\n")?;
        output_programs.write_all(programs.as_bytes())?;

        let year_spending = document
            .find(Name("script"))
            .filter(|n| n.html().contains("chartData"))
            .next()
            .map(|n| {
                let html = n.html();
                let chart_data_search = "var chartData = ";
                let chart_data_start = html.find(chart_data_search).unwrap() + chart_data_search.len();
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

use fantoccini::{ClientBuilder, Locator};
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
        "livestock" => livestock_data().await?,
        "spending" => spending_data().await?,
        s => panic!("Unknown data {s}")
    }

    Ok(())
}