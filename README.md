[![Workflow Status](https://github.com/japanese-law-analysis/listup_precedent/workflows/Rust%20CI/badge.svg)](https://github.com/japanese-law-analysis/listup_precedent/actions?query=workflow%3A%22Rust%2BCI%22)

# listup_precedent

裁判例のデータ一覧を[裁判所のホームページ](https://www.courts.go.jp/index.html)をスクレイピングして生成するソフトウェア

## Install

```sh
cargo install --git "https://github.com/japanese-law-analysis/listup_precedent.git"
```

## Use

```sh
listup_precedent --start "2022/01/12" --end "2023/12/01" --output "output.json"
```

のようにして使用します。すべて必須オプションです。

`--start`オプションと`--end`オプションにはそれぞれ`yyyy/mm/dd`形式の日付を与えます。
この２つの日付の間に判決が出た裁判例の情報を生成します。

`--output`オプションにはその生成した裁判例の情報を書き出すJSONファイルまでのpathを与えます。

## 生成される情報

以下のフィールドを持つオブジェクトの配列が生成されます。

### 必須フィールド

- trial_type: string `SupremeCourt`・`HighCourt`・`LowerCourt`・`AdministrativeCase`・`LaborCase`・`IPCase`のいずれか
- date: 裁判年月日
  - era: string `Showa`・`Heisei`・`Reiwa`のいずれか
  - era_year: int その元号の何年かを表す
  - year: int 西暦
  - month: int 月
  - day: int 日
- case_number: string 事件番号
- case_name: string 事件名
- court_name: string 裁判所・部・法廷名
- lawsuit_id: string 事件に振られているID
- detail_page_link: string 詳細が乗っているページのリンク
- full_page_link: string 判決文全文のPDFのリンク

### オプションフィールド

- right_skip: string 争われた対象の権利の種別
- lawsuit_type: string 訴訟類型
- result_type: string 判決の種別
- result: string 結果
- article_info: string 判例集等巻・号・頁
- original_court_name: string 原審裁判所名
- original_case_number: string 原審事件番号
- original_date: 原審裁判年月日
  - era: string `Showa`・`Heisei`・`Reiwa`のいずれか
  - era_year: int その元号の何年かを表す
  - year: int 西暦
  - month: int 月
  - day: int 日
- original_result: 原審結果
- field: string 分野
- gist: string 判事事項の要旨
- case_gis: string 裁判要旨
- ref_law: string 参照条文


---
[MIT License](https://github.com/japanese-law-analysis/listup_precedent/blob/master/LICENSE)
(c) 2023 Naoki Kaneko (a.k.a. "puripuri2100")


License: MIT
