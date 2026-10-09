//! Task-scoped one-pass text-evidence pilot, using existing Runner contracts.
//! Takes a JSON config path; stdout is a candidate manifest, not verification.
use std::{
  collections::BTreeMap,
  error::Error,
  fs,
  time::{Duration, Instant},
};

use auv_core::{
  AuvContext, Client,
  client::{RunnerOptions, runner::ScrollUntilEvent},
  runs::RunOutcome,
};
use auv_driver::{InputPolicy, Point, RecognizedText, Rect, RelativeRect, Scroll, ScrollOptions, TextRecognition, WindowPoint};
use auv_scan::{ScrollUntilCondition, ScrollUntilDecision, ScrollUntilOutputOptions, ScrollUntilRequest, ScrollUntilStep};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
struct Config {
  endpoint: String,
  device_id: String,
  window_id: String,
  queries: Vec<String>,
  point: Point,
  viewport: RelativeRect,
  step: f64,
  max_steps: u32,
  settle_ms: u64,
  /// `sequential` isolates one target per stream while retaining forward position.
  mode: String,
}

#[derive(Serialize)]
struct Evidence {
  query: String,
  candidate_text: String,
  row_bounds: Rect,
  path: String,
  receipt: serde_json::Value,
  step: u32,
  screenshot_verified: bool,
}

// OCR can split columns. Group by overlapping vertical centers, then order
// left-to-right. This is a pilot for single-line records, not a general table parser.
fn row_for_query(text: &TextRecognition, query: &str, viewport: Rect) -> Option<(String, Rect)> {
  let matches: Vec<_> = text.regions.iter().filter(|r| r.text.to_lowercase().contains(&query.to_lowercase())).collect();
  if matches.len() != 1 {
    return None;
  }
  let anchor = matches[0];
  let mut row: Vec<&RecognizedText> = text
    .regions
    .iter()
    .filter(|r| (r.bounds.center().y - anchor.bounds.center().y).abs() < r.bounds.size.height.min(anchor.bounds.size.height) * 0.5)
    .collect();
  row.sort_by(|a, b| a.bounds.origin.x.total_cmp(&b.bounds.origin.x));
  let x0 = row.iter().map(|r| r.bounds.origin.x).fold(f64::INFINITY, f64::min);
  let y0 = row.iter().map(|r| r.bounds.origin.y).fold(f64::INFINITY, f64::min);
  let x1 = row.iter().map(|r| r.bounds.origin.x + r.bounds.size.width).fold(f64::NEG_INFINITY, f64::max);
  let y1 = row.iter().map(|r| r.bounds.origin.y + r.bounds.size.height).fold(f64::NEG_INFINITY, f64::max);
  // Reject edge-clipped candidates. Visual verification still checks the fields;
  // OCR bounds cannot prove that every intended field was recognized.
  let margin = (y1 - y0) * 0.5;
  if x0 < viewport.origin.x
    || x1 > viewport.origin.x + viewport.size.width
    || y0 - margin < viewport.origin.y
    || y1 + margin > viewport.origin.y + viewport.size.height
  {
    return None;
  }
  Some((row.iter().map(|r| r.text.as_str()).collect::<Vec<_>>().join(" "), Rect::new(x0, y0, x1 - x0, y1 - y0)))
}

fn viewport(bounds: Rect, region: RelativeRect) -> Rect {
  Rect::new(
    bounds.origin.x + region.x * bounds.size.width,
    bounds.origin.y + region.y * bounds.size.height,
    region.width * bounds.size.width,
    region.height * bounds.size.height,
  )
}

fn signatures(text: &TextRecognition, area: Rect) -> Vec<String> {
  // Compare entire rows, not repeated status columns that can mask a gap.
  let mut rows: Vec<_> =
    text.regions.iter().filter_map(|r| row_for_query(text, &r.text, area)).map(|(line, _)| overlap_signature(&line)).collect();
  rows.sort();
  rows.dedup();
  rows
}

// OCR alternates between `|`, `I`, and `1` separators and splits hyphenated
// codes differently across frames. Normalize only coverage signatures;
// candidate text and persisted pixels retain their exact original values.
fn overlap_signature(line: &str) -> String {
  line
    .split_whitespace()
    .filter(|word| !["|", "I", "1"].contains(word))
    .flat_map(str::chars)
    .filter(|c| c.is_alphanumeric())
    .flat_map(char::to_lowercase)
    .collect()
}

#[tokio::main]
async fn main() -> Result<(), Box<dyn Error>> {
  let started = Instant::now();
  let path = std::env::args().nth(1).ok_or("usage: collect_scroll_targets CONFIG.json")?;
  let config: Config = serde_json::from_slice(&fs::read(path)?)?;
  if config.queries.is_empty()
    || config.queries.iter().any(|q| q.trim().is_empty())
    || !["one_pass", "sequential"].contains(&config.mode.as_str())
    || !(0.0..=1.0).contains(&config.point.x)
    || !(0.0..=1.0).contains(&config.point.y)
    || !config.viewport.is_normalized()
  {
    return Err("invalid collector config".into());
  }
  let client = Client::from_context(AuvContext {
    daemon_endpoint: Some(config.endpoint.clone()),
    device_id: Some(config.device_id.clone()),
    ..Default::default()
  })
  .await?;
  let execution = client.runner(RunnerOptions::default()).await?;
  let run_id = execution.run().id.clone();
  // Daemon placement Run IDs and tracing UUIDs are separate existing contracts.
  let recording_run_id = auv_tracing::RunId::new();
  let mut evidence = BTreeMap::new();
  let mut updates = Vec::new();
  let mut total_steps = 0;
  let mut streams = 0;
  let mut last_visible = Vec::new();
  let result: Result<serde_json::Value, Box<dyn Error>>=async {
    let window=execution.windows().get(&config.window_id).await?;
    let size=window.resource().frame.size;
    let point=WindowPoint::new(config.point.x*size.width, config.point.y*size.height);
    let mut request=ScrollUntilRequest { step:ScrollUntilStep::Instant{delta:Scroll::new(0.0,config.step)},
      condition:ScrollUntilCondition::End,max_steps:config.max_steps,settle:Duration::from_millis(config.settle_ms),
      no_motion_confirmations:2,motion_region:Some(config.viewport),output:ScrollUntilOutputOptions{text:true} };
    request.validate()?;
    let batches:Vec<Vec<String>>=if config.mode=="one_pass" {vec![config.queries.clone()]}
      else {config.queries.iter().map(|q|vec![q.clone()]).collect()};
    let mut previous=Vec::<String>::new();
    for batch in batches {
      if total_steps >= config.max_steps {break;}
      request.max_steps=config.max_steps-total_steps;
      streams+=1;
      let mut session=window.scroll_until(point,request.clone(),ScrollOptions{policy:InputPolicy::ForegroundPreferred,..Default::default()},true).await?;
      let mut completed=false;
      while let Some(event)=session.next().await? {
        match event {
          ScrollUntilEvent::Update{update,awaiting_decision}=> {
            let text=update.text.as_ref().ok_or("stream omitted requested OCR")?;
            let area=viewport(update.capture.bounds,config.viewport);
            let current=signatures(text,area);
            let overlap=current.iter().filter(|s|previous.contains(s)).count();
            // No blind gap recovery: partial evidence and the exact failure are
            // retained. Reopen adaptive backtracking only after this pilot.
            if update.steps>0 && !previous.is_empty() && overlap==0 {
              let (receipt,path)=execution.captures().record_artifact(&update.capture.reference,recording_run_id,"auv.pilot.scroll_overlap_failure").await?;
              updates.push(serde_json::json!({"stream":streams,"step":total_steps+update.steps,"text":text,
                "error":"viewport_overlap_lost","receipt":receipt,"path":path}));
              return Err(format!("viewport overlap lost at step {}",total_steps+update.steps).into());
            }
            last_visible=config.queries.iter().filter(|q|row_for_query(text,q,area).is_some()).cloned().collect();
            let new:Vec<_>=batch.iter().filter(|q|!evidence.contains_key(*q)).filter_map(|q|
              row_for_query(text,q,area).map(|(line,bounds)|(q.clone(),line,bounds))).collect();
            if !new.is_empty() {
              let (receipt,path)=execution.captures().record_artifact(&update.capture.reference,recording_run_id,"auv.pilot.scroll_target_evidence").await?;
              for (query,candidate_text,row_bounds) in new {
                evidence.insert(query.clone(),Evidence{query,candidate_text,row_bounds,path:path.to_string_lossy().into(),
                  receipt:serde_json::to_value(&receipt)?,step:total_steps+update.steps,screenshot_verified:false});
              }
            }
            updates.push(serde_json::json!({"stream":streams,"step":total_steps+update.steps,"overlap_regions":overlap,
              "capture":update.capture.reference.id(),"bounds":update.capture.bounds,"text":text,"stop":update.stop}));
            previous=current;
            if awaiting_decision {
              let found=batch.iter().all(|q|evidence.contains_key(q));
              session.decide(if found {ScrollUntilDecision::Stop}else{ScrollUntilDecision::Continue}).await?;
            }
          }
          ScrollUntilEvent::Completed(result)=> {
            total_steps+=result.steps;
            updates.push(serde_json::json!({"stream":streams,"completed":result}));
            completed=true;
          }
        }
      }
      if !completed {return Err("stream ended without completion".into());}
      if !batch.iter().all(|q|evidence.contains_key(q)) {break;}
    }
    Ok(serde_json::json!({"run_id":run_id.to_string(),"mode":config.mode,"steps":total_steps,"streams":streams,
      "records":config.queries.iter().filter_map(|q|evidence.get(q)).collect::<Vec<_>>(),"updates":&updates,
      "all_candidates_found":evidence.len()==config.queries.len(),"last_requested_visible":last_visible.contains(config.queries.last().unwrap()),
      "screenshot_verified":false}))
  }.await;
  let succeeded = result.as_ref().is_ok_and(|value| value["all_candidates_found"] == true);
  let cleanup = execution
    .finish(if succeeded {
      RunOutcome::Succeeded
    } else {
      RunOutcome::Failed
    })
    .await;
  let mut output = match result {
    Ok(value) => value,
    Err(error) => serde_json::json!({"run_id":run_id.to_string(),"error":error.to_string(),
    "records":evidence.values().collect::<Vec<_>>(),"updates":updates,"all_candidates_found":false,"screenshot_verified":false}),
  };
  output["collector_seconds"] = serde_json::json!(started.elapsed().as_secs_f64());
  if let Err(error) = cleanup {
    output["cleanup_error"] = serde_json::json!(error.to_string());
  }
  println!("{}", serde_json::to_string(&output)?);
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  fn region(text: &str, x: f64, y: f64) -> RecognizedText {
    RecognizedText {
      text: text.into(),
      bounds: Rect::new(x, y, 70.0, 18.0),
      confidence: None,
    }
  }
  #[test]
  fn split_columns_join_in_x_order_without_the_neighbor_row() {
    let text = TextRecognition {
      regions: vec![
        region("Approved", 230.0, 80.0),
        region("Target", 30.0, 80.0),
        region("AB-1234", 130.0, 80.0),
        region("Other", 30.0, 110.0),
      ],
      ..Default::default()
    };
    assert_eq!(row_for_query(&text, "Target", Rect::new(0.0, 0.0, 400.0, 200.0)).unwrap().0, "Target AB-1234 Approved");
  }
  #[test]
  fn clipped_rows_and_duplicate_matches_are_not_selected() {
    let mut text = TextRecognition {
      regions: vec![region("Target", 30.0, 0.0)],
      ..Default::default()
    };
    assert!(row_for_query(&text, "Target", Rect::new(0.0, 0.0, 400.0, 200.0)).is_none());
    text.regions = vec![region("Target", 30.0, 80.0), region("Target", 30.0, 110.0)];
    assert!(row_for_query(&text, "Target", Rect::new(0.0, 0.0, 400.0, 200.0)).is_none());
  }
  #[test]
  fn viewport_signatures_exclude_static_chrome_and_partial_rows() {
    let text = TextRecognition {
      regions: vec![
        region("Title", 30.0, 0.0),
        region("Row", 30.0, 80.0),
        region("Clipped", 30.0, 195.0),
      ],
      ..Default::default()
    };
    assert_eq!(signatures(&text, Rect::new(0.0, 40.0, 400.0, 160.0)), vec!["row"]);
  }
  #[test]
  fn separator_jitter_keeps_overlap_without_rewriting_candidate_values() {
    assert_eq!(overlap_signature("Archive record 007 | HN -6940 I Approved"), overlap_signature("Archive record 007 | HN 6940 | Approved"));
    assert_ne!(overlap_signature("Archive record 008 | HN-6940 | Approved"), overlap_signature("Archive record 007 | HN-6940 | Approved"));
  }
}
