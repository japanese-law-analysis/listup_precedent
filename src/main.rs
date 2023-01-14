//! 裁判例のデータ一覧を[裁判所のホームページ](https://www.courts.go.jp/index.html)をスクレイピングして生成するソフトウェア
//!

use anyhow::{anyhow, Result};
use clap::Parser;
//use log::*;
use regex::Regex;
use scraper::{Html, Selector};
use serde::{Deserialize, Serialize};
use tokio::{self, fs::*, io::AsyncWriteExt};
use tokio_stream::StreamExt;
use url::Url;

const COURTS_DOMEIN: &str = "https://www.courts.go.jp";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Era {
  Showa,
  Heisei,
  Reiwa,
}

async fn era_to_uri_encode(era: &Era) -> String {
  match era {
    Era::Showa => "%E6%98%AD%E5%92%8C".to_string(),
    Era::Heisei => "%E5%B9%B3%E6%88%90".to_string(),
    Era::Reiwa => "%E4%BB%A4%E5%92%8C".to_string(),
  }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Date {
  pub era: Era,
  pub era_year: usize,
  pub year: usize,
  pub month: usize,
  pub day: usize,
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

  let date_id = year * 10000 + month * 100 + day;

  if 12 < month || 31 < day {
    return Err(anyhow!("日付が範囲外です"));
  }

  if (19261225..=19890107).contains(&date_id) {
    Ok(Date {
      era: Era::Showa,
      era_year: year - 1925,
      year,
      month,
      day,
    })
  } else if (19890108..=20190430).contains(&date_id) {
    Ok(Date {
      era: Era::Heisei,
      era_year: year - 1988,
      year,
      month,
      day,
    })
  } else if 20190501 <= date_id {
    Ok(Date {
      era: Era::Reiwa,
      era_year: year - 2018,
      year,
      month,
      day,
    })
  } else {
    Err(anyhow!("日付が範囲外です"))
  }
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
      println!("v {:?}", v);
      return Err(anyhow!("元号が適切でない"));
    }
  };
  let year = match era {
    Era::Showa => era_year + 1925,
    Era::Heisei => era_year + 1988,
    Era::Reiwa => era_year + 2018,
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
    era_year,
    year,
    month,
    day,
  })
}

async fn get_reqest(start_date: &Date, end_date: &Date, page: usize) -> Result<String> {
  // https://www.courts.go.jp/app/hanrei_jp/list1?page={page}&sort=1&filter[judgeDateMode]=2&filter[judgeGengoFrom]={}&filter[judgeYearFrom]={}&filter[judgeMonthFrom]={}&filter[judgeDayFrom]={}&filter[judgeGengoTo]={}&filter[judgeYearTo]={}&filter[judgeMonthTo]={}&filter[judgeDayTo]={}
  let url_str = format!("{COURTS_DOMEIN}/app/hanrei_jp/list1?page={page}&sort=1&filter%5BjudgeDateMode%5D=2&filter%5BjudgeGengoFrom%5D={}&filter%5BjudgeYearFrom%5D={}&filter%5BjudgeMonthFrom%5D={}&filter%5BjudgeDayFrom%5D={}&filter%5BjudgeGengoTo%5D={}&filter%5BjudgeYearTo%5D={}&filter%5BjudgeMonthTo%5D={}&filter%5BjudgeDayTo%5D={}", era_to_uri_encode(&start_date.era).await, start_date.era_year, start_date.month, start_date.day, era_to_uri_encode(&end_date.era).await, end_date.era_year, end_date.month, end_date.day);
  let body = reqwest::get(url_str).await?.text().await?;
  Ok(body)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum TrialType {
  /// 最高裁判所
  SupremeCourt,
  /// 高等裁判所
  HighCourt,
  /// 下級裁判所
  LowerCourt,
  /// 行政事件
  AdministrativeCase,
  /// 労働事件
  LaborCase,
  /// 知的財産
  IPCase,
}

/// 判例集のページにあるフィールド
/// 具体例：
/// - 最高裁判所：https://www.courts.go.jp/app/hanrei_jp/detail2?id=91536
/// - 高等裁判所：https://www.courts.go.jp/app/hanrei_jp/detail3?id=91553
/// - 下級裁判所：https://www.courts.go.jp/app/hanrei_jp/detail4?id=91676
/// - 行政事件：https://www.courts.go.jp/app/hanrei_jp/detail5?id=91434
/// - 労働事件：https://www.courts.go.jp/app/hanrei_jp/detail6?id=90799
/// - 知的財産：https://www.courts.go.jp/app/hanrei_jp/detail7?id=91661
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PrecedentInfo {
  pub trial_type: TrialType,
  /// 裁判年月日
  pub date: Date,
  /// 事件番号
  pub case_number: String,
  /// 事件名
  pub case_name: String,
  /// 裁判所・部・法廷名
  pub court_name: String,
  /// 争われた対象の権利の種別
  #[serde(skip_serializing_if = "Option::is_none")]
  pub right_type: Option<String>,
  /// 訴訟類型
  #[serde(skip_serializing_if = "Option::is_none")]
  pub lawsuit_type: Option<String>,
  /// 裁判種別
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result_type: Option<String>,
  /// 結果
  #[serde(skip_serializing_if = "Option::is_none")]
  pub result: Option<String>,
  /// 判例集等巻・号・頁
  #[serde(skip_serializing_if = "Option::is_none")]
  pub article_info: Option<String>,
  /// 原審裁判所名
  #[serde(skip_serializing_if = "Option::is_none")]
  pub original_court_name: Option<String>,
  /// 原審事件番号
  #[serde(skip_serializing_if = "Option::is_none")]
  pub original_case_number: Option<String>,
  /// 分野
  #[serde(skip_serializing_if = "Option::is_none")]
  pub field: Option<String>,
  /// 判示事項の要旨
  #[serde(skip_serializing_if = "Option::is_none")]
  pub original_result: Option<String>,
  /// 判示事項の要旨
  #[serde(skip_serializing_if = "Option::is_none")]
  pub gist: Option<String>,
  /// 裁判要旨
  #[serde(skip_serializing_if = "Option::is_none")]
  pub case_gist: Option<String>,
  /// 参照法条
  #[serde(skip_serializing_if = "Option::is_none")]
  pub ref_law: Option<String>,
  /// 事件に振られているID
  pub lawsuit_id: String,
  /// 詳細が乗っているページ
  pub detail_page_link: String,
  /// 判決文前文
  pub full_pdf_link: String,
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

#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
struct Args {
  /// 解析結果を出力するJSONファイルへのpath
  #[clap(short, long)]
  output: String,
  /// 取得したい判例の日時の開始 yyyy/mm/dd形式で記述
  #[clap(short, long)]
  start: String,
  /// 取得したい判例の日時の終了 yyyy/mm/dd形式で記述
  #[clap(short, long)]
  end: String,
}

//async fn init_logger() -> Result<()> {
//  let subscriber = tracing_subscriber::fmt()
//    .with_max_level(tracing::Level::INFO)
//    .finish();
//  tracing::subscriber::set_global_default(subscriber)?;
//  Ok(())
//}

#[tokio::main]
async fn main() -> Result<()> {
  let args = Args::parse();
  //init_logger().await?;

  let start_date = parse_date(&args.start).await?;
  let end_date = parse_date(&args.end).await?;

  println!("start_date: {}", &args.start);
  println!("end_date: {}", &args.end);

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
  let mut output_file = File::create(file_path).await?;
  let mut is_head = true;
  println!("[START] writing file: {}", &file_path);
  output_file.write_all("[".as_bytes()).await?;
  while let Some(page_num) = stream.next().await {
    println!("page_num: {}", page_num);
    let html = get_reqest(&start_date, &end_date, page_num).await?;
    println!("html ok");
    let page_document = Html::parse_document(&html);
    let detail_page_link_selector = Selector::parse("table > tbody > tr > th > a").unwrap();
    let mut detail_page_link_stream =
      tokio_stream::iter(page_document.select(&detail_page_link_selector));
    while let Some(detail_page_link) = detail_page_link_stream.next().await {
      let link = detail_page_link
        .value()
        .attr("href")
        .expect("a属性はhrefを持っているはず");
      println!("link: {}", &link);
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
      println!("[START] date write: {}", &lawsuit_id);
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
          _ => println!("!!! OTHER: {}", &dt_text),
        }
      }
      let date = parse_date_era_str(&date_str).await?;
      let precedent_info = PrecedentInfo {
        trial_type,
        date,
        case_number,
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
        field,
        gist,
        case_gist,
        ref_law,
        lawsuit_id: lawsuit_id.clone(),
        detail_page_link,
        full_pdf_link,
      };
      let precedent_info_json_str = serde_json::to_string(&precedent_info)?;
      if is_head {
        output_file.write_all("\n".as_bytes()).await?;
        is_head = false;
      } else {
        output_file.write_all(",\n".as_bytes()).await?;
      }
      output_file
        .write_all(precedent_info_json_str.as_bytes())
        .await?;
      println!("[END] date write: {}", &lawsuit_id);
    }

    if is_head {
      is_head = false
    }
  }
  output_file.write_all("\n]".as_bytes()).await?;
  output_file.flush().await?;
  println!("[END] write json file");
  Ok(())
}
