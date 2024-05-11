//! 裁判例のデータ一覧を[裁判所のホームページ](https://www.courts.go.jp/index.html)をスクレイピングして生成するソフトウェア
//!
//! # Install
//!
//! ```sh
//! cargo install --git "https://github.com/japanese-law-analysis/listup_precedent.git"
//! ```
//!
//! # Use
//!
//! ```sh
//! listup_precedent --start "2022/01/12" --end "2023/12/01" --output "output" --index "output/list.json"
//! ```
//!
//! のようにして使用します。すべて必須オプションです。
//!
//! `--start`オプションと`--end`オプションにはそれぞれ`yyyy/mm/dd`形式の日付を与えます。
//! この２つの日付の間に判決が出た裁判例の情報を生成します。
//!
//! - `--output`オプションにはその生成した裁判例の情報を書き出すフォルダのpathを与えます。
//! - `--index`オプションには裁判例情報の一覧を書き出すJSONファイルのpathを与えます。
//!
//! # 生成される情報
//!
//! 以下のフィールドを持つオブジェクトの配列が生成されます。
//!
//! ## 必須フィールド
//!
//! - trial_type: string `SupremeCourt`・`HighCourt`・`LowerCourt`・`AdministrativeCase`・`LaborCase`・`IPCase`のいずれか
//! - date: 裁判年月日
//!   - era: string `Showa`・`Heisei`・`Reiwa`のいずれか
//!   - era_year: int その元号の何年かを表す
//!   - year: int 西暦
//!   - month: int 月
//!   - day: int 日
//! - case_number: string 事件番号
//! - case_name: string 事件名
//! - court_name: string 裁判所・部・法廷名
//! - lawsuit_id: string 事件に振られているID
//! - detail_page_link: string 詳細が乗っているページのリンク
//! - full_page_link: string 判決文全文のPDFのリンク
//!
//! ## オプションフィールド
//!
//! - right_skip: string 争われた対象の権利の種別
//! - lawsuit_type: string 訴訟類型
//! - result_type: string 判決の種別
//! - result: string 結果
//! - article_info: string 判例集等巻・号・頁
//! - original_court_name: string 原審裁判所名
//! - original_case_number: string 原審事件番号
//! - original_date: 原審裁判年月日
//!   - era: string `Showa`・`Heisei`・`Reiwa`のいずれか
//!   - era_year: int その元号の何年かを表す
//!   - year: int 西暦
//!   - month: int 月
//!   - day: int 日
//! - original_result: 原審結果
//! - field: string 分野
//! - gist: string 判事事項の要旨
//! - case_gis: string 裁判要旨
//! - ref_law: string 参照条文
//!
//!
//! ---
//! [MIT License](https://github.com/japanese-law-analysis/listup_precedent/blob/master/LICENSE)
//! (c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")
//!

use anyhow::{anyhow, Result};
use clap::Parser;
use japanese_law_xml_schema::law::Era;
use jplaw_data_types::{
  law::Date,
  listup::{PrecedentData, PrecedentInfo},
  precedent::TrialType,
};
use jplaw_io::{flush_file_value_lst, gen_file_value_lst, init_logger, write_value_lst};
use jplaw_pdf2text::{clean_up, pdf_bytes_to_text};
use regex::Regex;
use scraper::{Html, Selector};
use tokio::{self, fs::*, io::AsyncWriteExt};
use tokio_stream::StreamExt;
use tracing::*;
use url::Url;

const COURTS_DOMEIN: &str = "https://www.courts.go.jp";

async fn era_to_uri_encode(era: &Era) -> String {
  match era {
    Era::Showa => "%E6%98%AD%E5%92%8C".to_string(),
    Era::Heisei => "%E5%B9%B3%E6%88%90".to_string(),
    Era::Reiwa => "%E4%BB%A4%E5%92%8C".to_string(),
    _ => unreachable!(),
  }
}

async fn parse_date(str: &str) -> Result<Date> {
  let mut chars = str.chars();

  let year_str = chars.by_ref().take(4).collect::<String>();

  let year = year_str.parse::<usize>()?;

  let _ = chars.by_ref().take(1).collect::<String>();

  let month_str = chars.by_ref().take(2).collect::<String>();

  let month = month_str.parse::<usize>()?;

  let _ = chars.by_ref().take(1).collect::<String>();

  let day_str = chars.by_ref().take(2).collect::<String>();

  let day = day_str.parse::<usize>()?;

  if 12 < month || 31 < day {
    return Err(anyhow!("日付が範囲外です"));
  }

  Ok(Date::gen_from_ad(year, month, day))
}

async fn parse_date_era_str(str: &str) -> Result<Date> {
  let re =
    Regex::new(r"(?P<era>[^0-9]+)(?P<era_year>\d+)年(?P<month>\d+)月(?P<day>\d+)日").unwrap();
  let re_gan = Regex::new(r"(?P<era>[^0-9]+)元年(?P<month>\d+)月(?P<day>\d+)日").unwrap();
  let (caps, era_year) = match re.captures(str) {
    Some(caps) => {
      let era_year = caps
        .name("era_year")
        .map(|v| v.as_str())
        .ok_or_else(|| anyhow!("年号付き日付のパースに失敗（年）"))?
        .parse::<usize>()?;
      (caps, era_year)
    }
    None => {
      let caps = re_gan
        .captures(str)
        .ok_or_else(|| anyhow!("年号付き日付のパースに失敗：{}", str))?;
      (caps, 1)
    }
  };
  let era = match caps.name("era").map(|v| v.as_str()) {
    Some("昭和") => Era::Showa,
    Some("平成") => Era::Heisei,
    Some("令和") => Era::Reiwa,
    v => {
      info!("v {:?}", v);
      return Err(anyhow!("元号が適切でない"));
    }
  };
  let month = caps
    .name("month")
    .map(|v| v.as_str())
    .ok_or_else(|| anyhow!("年号付き日付のパースに失敗（月）"))?
    .parse::<usize>()?;
  let day = caps
    .name("day")
    .map(|v| v.as_str())
    .ok_or_else(|| anyhow!("年号付き日付のパースに失敗（日）"))?
    .parse::<usize>()?;
  Ok(Date {
    era,
    year: era_year,
    month: Some(month),
    day: Some(day),
  })
}

async fn get_reqest(start_date: &Date, end_date: &Date, page: usize) -> Result<String> {
  // https://www.courts.go.jp/app/hanrei_jp/list1?page={page}&sort=1&filter[judgeDateMode]=2&filter[judgeGengoFrom]={}&filter[judgeYearFrom]={}&filter[judgeMonthFrom]={}&filter[judgeDayFrom]={}&filter[judgeGengoTo]={}&filter[judgeYearTo]={}&filter[judgeMonthTo]={}&filter[judgeDayTo]={}
  let url_str = format!("{COURTS_DOMEIN}/app/hanrei_jp/list1?page={page}&sort=1&filter%5BjudgeDateMode%5D=2&filter%5BjudgeGengoFrom%5D={}&filter%5BjudgeYearFrom%5D={}&filter%5BjudgeMonthFrom%5D={}&filter%5BjudgeDayFrom%5D={}&filter%5BjudgeGengoTo%5D={}&filter%5BjudgeYearTo%5D={}&filter%5BjudgeMonthTo%5D={}&filter%5BjudgeDayTo%5D={}", era_to_uri_encode(&start_date.era).await, start_date.year, start_date.month.unwrap_or_default(), start_date.day.unwrap_or_default(), era_to_uri_encode(&end_date.era).await, end_date.year, end_date.month.unwrap_or_default(), end_date.day.unwrap_or_default());
  let body = reqwest::get(url_str).await?.text().await?;
  Ok(body)
}

async fn get_pdf_text(pdf_link: &str) -> Result<String> {
  let bytes = reqwest::get(pdf_link).await?.bytes().await?;
  let text = pdf_bytes_to_text(&bytes)?;
  let text = clean_up(&text);
  Ok(text)
}

async fn get_lawsuit_id(url_str: &str) -> Result<String> {
  let url = Url::parse(url_str)?;
  let mut querys = url.query_pairs();
  let id = querys.next().ok_or_else(|| anyhow!("リンクにidが無い"))?.1;
  Ok(id.to_string())
}

fn remove_line_break(str: &str) -> String {
  str.lines().map(|s| s.trim()).collect::<String>()
}

async fn write_data(output: &str, filename: &str, data: &PrecedentData) -> Result<()> {
  let mut buf = File::create(format!("{output}/{filename}.json")).await?;
  let s = serde_json::to_string_pretty(&data)?;
  buf.write_all(s.as_bytes()).await?;
  buf.flush().await?;
  Ok(())
}

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// 解析結果を出力するJSONファイルへのpath
  #[clap(short, long)]
  output: String,
  /// 一覧を出力するJSONファイル名
  #[clap(short, long)]
  index: String,
  /// 取得したい判例の日時の開始 yyyy/mm/dd形式で記述
  #[clap(short, long)]
  start: String,
  /// 取得したい判例の日時の終了 yyyy/mm/dd形式で記述
  #[clap(short, long)]
  end: String,
  /// 一回のrowについてのAPIアクセスが行われるたびにsleepする時間（ミリ秒）
  #[clap(short, long, default_value = "500")]
  sleep_time: u64,
}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();
  init_logger().await?;

  let start_date = parse_date(&args.start).await?;
  let end_date = parse_date(&args.end).await?;

  info!("start_date: {}", &args.start);
  info!("end_date: {}", &args.end);

  let top_html = get_reqest(&start_date, &end_date, 1).await?;
  let top_document = Html::parse_document(&top_html);
  let all_quantity_selector = Selector::parse("div.module-search-page-paging-parts2 > p").unwrap();
  // "64297件中11～20件を表示"のような値になっている
  let all_quantity_text = top_document
    .select(&all_quantity_selector)
    .next()
    .unwrap()
    .text()
    .collect::<String>();
  let re = Regex::new(r"\d+").unwrap();
  let all_quantity = &re.captures(&all_quantity_text).unwrap()[0].parse::<usize>()?;
  let all_page_quantity = all_quantity / 10;
  let all_page_quantity = if all_quantity % 10 == 0 {
    all_page_quantity
  } else {
    all_page_quantity + 1
  };
  let mut stream = tokio_stream::iter(1..=all_page_quantity);
  let link_re = Regex::new(r"[^\d]+(?P<type_number>\d).*").unwrap();
  let file_path = &args.output;
  let mut index_file = gen_file_value_lst(&args.index).await?;
  info!("[START] writing file: {}", &file_path);
  while let Some(page_num) = stream.next().await {
    info!("page_num: {}", page_num);
    let html = get_reqest(&start_date, &end_date, page_num).await?;
    info!("html ok");
    let page_document = Html::parse_document(&html);
    let detail_page_link_selector = Selector::parse("table > tbody > tr > th > a").unwrap();
    let mut detail_page_link_stream =
      tokio_stream::iter(page_document.select(&detail_page_link_selector));
    while let Some(detail_page_link) = detail_page_link_stream.next().await {
      let link = detail_page_link
        .value()
        .attr("href")
        .expect("a属性はhrefを持っているはず");
      info!("link: {}", &link);
      let trial_type = match link_re
        .captures(link)
        .ok_or_else(|| anyhow!("年号付き日付のパースに失敗"))?
        .name("type_number")
        .ok_or_else(|| anyhow!("リンクが想定外の形をしている"))?
        .as_str()
        .parse::<usize>()?
      {
        2 => TrialType::SupremeCourt,
        3 => TrialType::HighCourt,
        4 => TrialType::LowerCourt,
        5 => TrialType::AdministrativeCase,
        6 => TrialType::LaborCase,
        7 => TrialType::IPCase,
        _ => unreachable!(),
      };
      let detail_page_link = format!("{COURTS_DOMEIN}{link}");
      let lawsuit_id = get_lawsuit_id(&detail_page_link).await?;
      info!("[START] date write: {}", &lawsuit_id);
      let detail_page_html = reqwest::get(&detail_page_link).await?.text().await?;
      let detail_document = Html::parse_document(&detail_page_html);
      let info_selector =
        Selector::parse("div.module-search-page-table-parts-result-detail > dl").unwrap();
      let mut date_str = String::new();
      let mut case_number = String::new();
      let mut case_name = String::new();
      let mut court_name = String::new();
      let mut right_type = None;
      let mut lawsuit_type = None;
      let mut result_type = None;
      let mut result = None;
      let mut article_info = None;
      let mut original_court_name = None;
      let mut original_case_number = None;
      let mut original_result = None;
      let mut original_date = None;
      let mut field = None;
      let mut gist = None;
      let mut case_gist = None;
      let mut ref_law = None;
      let mut full_pdf_link = String::new();
      let mut info_stream = tokio_stream::iter(detail_document.select(&info_selector));
      while let Some(info_element) = info_stream.next().await {
        let dt_selector = Selector::parse("dt").unwrap();
        let dd_text_selector = Selector::parse("dd > p").unwrap();
        let dd_link_selector = Selector::parse("dd > ul > li > a").unwrap();
        let dt_text = info_element
          .select(&dt_selector)
          .next()
          .unwrap()
          .text()
          .collect::<String>()
          .trim()
          .to_string();
        match &*dt_text {
          "事件番号" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            case_number = text;
          }
          "事件名" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            case_name = text;
          }
          "裁判年月日" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            date_str = text;
          }
          "裁判所名" | "裁判所名・部" | "法廷名" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            court_name = remove_line_break(&text);
          }
          "権利種別" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              right_type = Some(text);
            }
          }
          "訴訟類型" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              lawsuit_type = Some(text);
            }
          }
          "裁判種別" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              result_type = Some(text);
            }
          }
          "結果" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              result = Some(text);
            }
          }
          "判例集等巻・号・頁" | "高裁判例集登載巻・号・頁" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              article_info = Some(text);
            }
          }
          "原審裁判所名" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              original_court_name = Some(text);
            }
          }
          "原審事件番号" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              original_case_number = Some(text);
            }
          }
          "原審結果" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              original_result = Some(text);
            }
          }
          "原審裁判年月日" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              let date = parse_date_era_str(&text).await?;
              original_date = Some(date);
            }
          }
          "分野" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              field = Some(text);
            }
          }
          "判示事項の要旨" | "判示事項" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              gist = Some(text);
            }
          }
          "裁判要旨" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              case_gist = Some(text);
            }
          }
          "参照法条" => {
            let text = info_element
              .select(&dd_text_selector)
              .next()
              .unwrap()
              .text()
              .collect::<String>()
              .trim()
              .to_string();
            if !text.is_empty() {
              ref_law = Some(text);
            }
          }
          "全文" => {
            let link = info_element
              .select(&dd_link_selector)
              .next()
              .unwrap()
              .value()
              .attr("href")
              .expect("a属性はhrefを持っているはず");
            full_pdf_link = format!("{COURTS_DOMEIN}{link}");
          }
          _ => info!("!!! OTHER: {}", &dt_text),
        }
      }
      let date = parse_date_era_str(date_str.trim()).await?;
      let precedent_data = PrecedentData {
        trial_type: trial_type.clone(),
        date: date.clone(),
        case_number: case_number.clone(),
        case_name,
        court_name,
        right_type,
        lawsuit_type,
        result_type,
        result,
        article_info,
        original_court_name,
        original_case_number,
        original_result,
        original_date,
        field,
        gist,
        case_gist,
        ref_law,
        lawsuit_id: lawsuit_id.clone(),
        detail_page_link,
        contents: get_pdf_text(&full_pdf_link).await.ok(),
        full_pdf_link,
      };
      let precedent_info = PrecedentInfo {
        case_number: precedent_data.case_number.clone(),
        court_name: precedent_data.court_name.clone(),
        trial_type: precedent_data.trial_type.clone(),
        date: precedent_data.date.clone(),
        lawsuit_id: precedent_data.lawsuit_id.clone(),
      };
      let file_name = precedent_info.file_name();
      write_data(&args.output, &file_name, &precedent_data).await?;
      write_value_lst(&mut index_file, &precedent_info).await?;
      info!("[END] date write: {}", &lawsuit_id);
    }
    // 負荷を抑えるために500ミリ秒待つ
    info!("sleep");
    tokio::time::sleep(tokio::time::Duration::from_millis(args.sleep_time)).await;
  }
  flush_file_value_lst(&mut index_file).await?;
  info!("[END] write json file");
  Ok(())
}
