//! Native provider frames -> Orca-style revisioned journal, wholly in Rust.
use super::*;
pub(super) fn event(rt: &NativeRuntime, s: &Session, f: &Value) -> Result<(), Error> {
    if s.agent == "codex" {
        codex(rt, s, f)
    } else {
        claude(rt, s, f)
    }
}
fn message(role: &str, text: &str) -> Value {
    json!({"kind":"message","role":role,"blocks":[{"type":"text","text":text}]})
}
fn codex(rt: &NativeRuntime, s: &Session, f: &Value) -> Result<(), Error> {
    let method = f["method"].as_str().unwrap_or_default();
    let p = &f["params"];
    // A child's subagent notifications must not change its root turn or prompts.
    if p.get("threadId").is_some() && p["threadId"] != s.provider_id {
        return Ok(());
    }
    if f.get("id").is_some() && !method.is_empty() {
        let key = format!(
            "prompt:{}:{}",
            s.generation,
            hex::encode(f["id"].to_string())
        );
        let mut body = json!({"resolution":{"state":"pending","selectedOptionId":null},"generation":s.generation,"request":f["id"],"method":method});
        match method {
            "item/commandExecution/requestApproval" | "item/fileChange/requestApproval" => {
                body["kind"] = json!("approval");
                body["title"] = json!(if method.contains("commandExecution") {
                    "Run command"
                } else {
                    "Apply file changes"
                });
                // Preserve every scope-bearing field, including network/extra permissions.
                body["detail"] = json!(serde_json::to_string_pretty(p).unwrap_or_default());
                let offered = p["availableDecisions"].as_array();
                let options: Vec<_> = [
                    ("accept", "Allow once"),
                    ("decline", "Deny"),
                    ("cancel", "Cancel turn"),
                ]
                .into_iter()
                .filter(|(v, _)| offered.is_none_or(|o| o.contains(&json!(v))))
                .map(|(id, label)| json!({"id":id,"label":label}))
                .collect();
                body["options"] = json!(options);
            }
            "item/tool/requestUserInput" => {
                body["kind"] = json!("question");
                body["question"] = json!("Agent questions");
                let mut questions = Vec::new();
                for q in p["questions"]
                    .as_array()
                    .ok_or_else(|| unavailable("Invalid CLI question"))?
                {
                    let options: Vec<_> = q["options"].as_array().into_iter().flatten().enumerate().map(|(i,o)| json!({"id":i.to_string(),"label":o["label"],"description":o["description"]})).collect();
                    questions.push(json!({"id":q["id"],"question":q["question"],"header":q["header"],"multiSelect":false,"options":options,"freeTextQuestionId":if q["isOther"]==true{q["id"].clone()}else{Value::Null}}));
                }
                body["questions"] = json!(questions);
            }
            _ => {
                body["kind"] = json!("status");
                body["text"] = json!(format!(
                    "Unsupported CLI request: {method}. Interrupt this turn to continue."
                ));
                body["resolution"] = Value::Null;
            }
        }
        return rt.item(&s.id, &key, body);
    }
    match method {
        "turn/started" => rt.update(&s.id, |s| { s.turn = p["turn"]["id"].as_str().unwrap_or_default().into(); s.state = "running".into(); }),
        "turn/completed" => {
            rt.update(&s.id, |s| { s.state = match p["turn"]["status"].as_str() { Some("completed")=>"completed", Some("interrupted")=>"interrupted", _=>"unverifiable" }.into(); s.turn.clear(); })?;
            rt.cancel_prompts(&s.id)?;
            rt.item(&s.id,&format!("turn:{}",p["turn"]["id"].as_str().unwrap_or("unknown")),json!({"kind":"turn","state":p["turn"]["status"],"error":p["turn"]["error"]}))
        }
        "serverRequest/resolved" => {
            let key = format!("prompt:{}:{}",s.generation,hex::encode(p["requestId"].to_string()));
            if let Some(item)=rt.items(&s.id)?.iter().find(|i|i["itemId"]==key) { let mut body=item["body"].clone(); if body["resolution"]["state"]=="pending" { body["resolution"]["state"]=json!("cancelled");rt.item(&s.id,&key,body)?; } } Ok(())
        }
        "item/started" | "item/completed" => {
            let item=&p["item"];let key=format!("{}:{}",p["turnId"].as_str().unwrap_or(&s.turn),item["id"].as_str().unwrap_or("item"));
            let body=match item["type"].as_str() {
                Some("userMessage")=>return Ok(()),
                Some("agentMessage"|"plan")=>message("assistant",item["text"].as_str().unwrap_or_default()),
                Some("reasoning")=>message("reasoning",&item["summary"].as_array().map(|a|a.iter().filter_map(Value::as_str).collect::<Vec<_>>().join("\n")).unwrap_or_default()),
                Some("commandExecution")=>json!({"kind":"tool-call","name":"shell","callId":key,"input":item["command"],"state":if item["exitCode"].as_i64().is_some_and(|code| code != 0) { json!("failed") } else { item["status"].clone() },"output":{"head":item["aggregatedOutput"].as_str().unwrap_or_default(),"truncated":false}}),
                Some("fileChange")=>json!({"kind":"tool-call","name":"file changes","callId":key,"input":item["changes"],"state":item["status"]}),
                Some("mcpToolCall"|"dynamicToolCall"|"collabToolCall")=>json!({"kind":"tool-call","name":item["tool"],"callId":key,"input":item["arguments"],"state":item["status"],"output":{"head":item["result"].to_string(),"truncated":false}}),
                _=>json!({"kind":"status","text":format!("{}: {}",item["type"].as_str().unwrap_or("CLI activity"),item)})
            };rt.item(&s.id,&key,body)
        }
        "item/agentMessage/delta" | "item/plan/delta" | "item/reasoning/summaryTextDelta" => {
            let key=format!("{}:{}",p["turnId"].as_str().unwrap_or(&s.turn),p["itemId"].as_str().unwrap_or("message"));
            let mut body=rt.items(&s.id)?.into_iter().find(|i|i["itemId"]==key).map(|i|i["body"].clone()).unwrap_or_else(||message(if method.contains("reasoning"){"reasoning"}else{"assistant"},""));
            let text=format!("{}{}",body["blocks"][0]["text"].as_str().unwrap_or_default(),p["delta"].as_str().unwrap_or_default());body["blocks"][0]["text"]=json!(text);rt.item(&s.id,&key,body)
        }
        "turn/diff/updated"=>rt.item(&s.id,&format!("diff:{}",p["turnId"].as_str().unwrap_or(&s.turn)),json!({"kind":"diff","path":"Turn changes","patch":{"head":p["diff"],"truncated":false}})),
        "error"=>rt.item(&s.id,&format!("error:{}",random()),json!({"kind":"status","text":diagnostic::error_text(&p["error"]).unwrap_or_else(||"CLI agent reported an error; inspect its sign-in, limits and configuration.".into())})),
        _=>Ok(())
    }
}
fn claude(rt: &NativeRuntime, s: &Session, f: &Value) -> Result<(), Error> {
    match f["type"].as_str() {
        Some("stream_event") => {
            let event = &f["event"];
            match event["type"].as_str() {
                Some("message_start") => {
                    let message_id = string(&event["message"], "id")?;
                    rt.update(&s.id, |s| s.stream_message = message_id)
                }
                Some("content_block_start") if !s.stream_message.is_empty() => {
                    let key = format!(
                        "{}:{}",
                        s.stream_message,
                        event["index"].as_u64().unwrap_or(0)
                    );
                    let block = &event["content_block"];
                    let body = match block["type"].as_str() {
                        Some("text") => {
                            message("assistant", block["text"].as_str().unwrap_or_default())
                        }
                        Some("thinking") => {
                            message("reasoning", block["thinking"].as_str().unwrap_or_default())
                        }
                        Some("tool_use") => {
                            json!({"kind":"tool-call","name":block["name"],"callId":block["id"],"input":block["input"],"state":"running"})
                        }
                        _ => return Ok(()),
                    };
                    rt.item(&s.id, &key, body)
                }
                Some("content_block_delta") if !s.stream_message.is_empty() => {
                    let key = format!(
                        "{}:{}",
                        s.stream_message,
                        event["index"].as_u64().unwrap_or(0)
                    );
                    let Some(item) = rt.items(&s.id)?.into_iter().find(|i| i["itemId"] == key)
                    else {
                        return Ok(());
                    };
                    let mut body = item["body"].clone();
                    let delta = &event["delta"];
                    match delta["type"].as_str() {
                        Some("text_delta" | "thinking_delta") => {
                            let text = delta["text"]
                                .as_str()
                                .or(delta["thinking"].as_str())
                                .unwrap_or_default();
                            body["blocks"][0]["text"] = json!(format!(
                                "{}{}",
                                body["blocks"][0]["text"].as_str().unwrap_or_default(),
                                text
                            ));
                        }
                        Some("input_json_delta") => {
                            let partial = format!(
                                "{}{}",
                                body["partial"].as_str().unwrap_or_default(),
                                delta["partial_json"].as_str().unwrap_or_default()
                            );
                            body["input"] =
                                serde_json::from_str(&partial).unwrap_or(json!(partial));
                            body["partial"] = json!(partial);
                        }
                        _ => return Ok(()),
                    }
                    rt.item(&s.id, &key, body)
                }
                _ => Ok(()),
            }
        }
        Some("system") if f["subtype"] == "init" => {
            let id = string(f, "session_id")?;
            rt.update(&s.id, |s| s.provider_id = id)
        }
        Some("assistant" | "user") => {
            let key = f["message"]["id"]
                .as_str()
                .or(f["uuid"].as_str())
                .unwrap_or("message");
            for (index, block) in f["message"]["content"]
                .as_array()
                .into_iter()
                .flatten()
                .enumerate()
            {
                let key = format!("{key}:{index}");
                let body = match block["type"].as_str() {
                    Some("text") if f["type"] == "assistant" => {
                        message("assistant", block["text"].as_str().unwrap_or_default())
                    }
                    Some("thinking") => {
                        message("reasoning", block["thinking"].as_str().unwrap_or_default())
                    }
                    Some("tool_use") => {
                        json!({"kind":"tool-call","name":block["name"],"callId":block["id"],"input":block["input"],"state":"running"})
                    }
                    Some("tool_result") => {
                        json!({"kind":"message","role":"tool","blocks":[{"type":"tool-result","toolUseId":block["tool_use_id"],"state":if block["is_error"]==true{"failed"}else{"completed"},"text":block["content"].as_str().map(str::to_owned).unwrap_or_else(||block["content"].to_string())}]})
                    }
                    _ => continue,
                };
                rt.item(&s.id, &key, body)?;
            }
            Ok(())
        }
        Some("result") => {
            rt.update(&s.id, |s| {
                s.state = if f["is_error"] == true {
                    "unverifiable"
                } else {
                    "completed"
                }
                .into();
                s.turn.clear();
            })?;
            rt.cancel_prompts(&s.id)?;
            rt.item(
                &s.id,
                &format!("turn:{}", f["uuid"].as_str().unwrap_or(&random())),
                json!({"kind":"turn","state":if f["is_error"]==true{"failed"}else{"completed"}}),
            )
        }
        Some("control_request") => {
            let request = &f["request"];
            let key = format!(
                "prompt:{}:{}",
                s.generation,
                hex::encode(f["request_id"].to_string())
            );
            if request["subtype"] != "can_use_tool" {
                return rt.item(&s.id,&key,json!({"kind":"status","text":"Unsupported Claude control request. Interrupt this turn to continue."}));
            }
            let mut body = json!({"kind":"approval","title":request["tool_name"],"detail":serde_json::to_string_pretty(&request["input"]).unwrap_or_default(),"options":[{"id":"allow","label":"Allow once"},{"id":"deny","label":"Deny"}],"resolution":{"state":"pending","selectedOptionId":null},"request":f["request_id"],"input":request["input"],"generation":s.generation});
            if request["tool_name"] == "AskUserQuestion" {
                let questions:Vec<_>=request["input"]["questions"].as_array().ok_or_else(||unavailable("Invalid Claude question"))?.iter().enumerate().map(|(i,q)| {
                    let options:Vec<_>=q["options"].as_array().into_iter().flatten().enumerate().map(|(j,o)|json!({"id":j.to_string(),"label":o["label"],"description":o["description"]})).collect();
                    json!({"id":i.to_string(),"question":q["question"],"header":q["header"],"multiSelect":q["multiSelect"],"options":options,"freeTextQuestionId":i.to_string()})
                }).collect();
                body["kind"] = json!("question");
                body["questions"] = json!(questions);
                body["question"] = json!("Claude questions");
            }
            rt.item(&s.id, &key, body)
        }
        Some("control_cancel_request") => {
            let key = format!(
                "prompt:{}:{}",
                s.generation,
                hex::encode(f["request_id"].to_string())
            );
            if let Some(item) = rt.items(&s.id)?.iter().find(|i| i["itemId"] == key) {
                let mut body = item["body"].clone();
                body["resolution"]["state"] = json!("cancelled");
                rt.item(&s.id, &key, body)?;
            }
            Ok(())
        }
        _ => Ok(()),
    }
}
pub(super) fn response(s: &Session, body: &Value, params: &Value) -> Result<Value, Error> {
    let response = if body["kind"] == "question" {
        let questions = body["questions"]
            .as_array()
            .ok_or_else(|| unavailable("Unsupported question"))?;
        let answers = params["answers"]
            .as_array()
            .ok_or_else(|| Error::Invalid("Answer each question".into()))?;
        if questions.len() != answers.len() {
            return Err(Error::Invalid("Answer each question exactly once".into()));
        }
        let mut mapped = serde_json::Map::new();
        for q in questions {
            let found: Vec<_> = answers
                .iter()
                .filter(|a| a["questionId"] == q["id"])
                .collect();
            if found.len() != 1 {
                return Err(Error::Invalid("Question changed".into()));
            }
            let answer = found[0];
            let options = answer["optionIds"]
                .as_array()
                .ok_or_else(|| Error::Invalid("Invalid answer".into()))?;
            if q["multiSelect"] != true && options.len() > 1 {
                return Err(Error::Invalid("Choose one option".into()));
            }
            let mut labels = Vec::new();
            for option in options {
                let offered = q["options"]
                    .as_array()
                    .and_then(|opts| opts.iter().find(|o| o["id"] == *option))
                    .ok_or_else(|| Error::Invalid("Unknown answer option".into()))?;
                labels.push(string(offered, "label")?);
            }
            let other = answer["other"].as_str().unwrap_or_default();
            if !other.is_empty() {
                if q["freeTextQuestionId"].is_null() || other.len() > 64 * 1024 {
                    return Err(Error::Invalid(
                        "Free text is unavailable or too long".into(),
                    ));
                }
                labels.push(other.into());
            }
            if labels.is_empty() {
                return Err(Error::Invalid("An answer is required".into()));
            }
            if s.agent == "codex" {
                mapped.insert(string(q, "id")?, json!({"answers":labels}));
            } else {
                mapped.insert(string(q, "question")?, json!(labels.join(", ")));
            }
        }
        if s.agent == "codex" {
            json!({"answers":mapped})
        } else {
            let mut input = body["input"].clone();
            input["answers"] = json!(mapped);
            json!({"behavior":"allow","updatedInput":input})
        }
    } else {
        let option = string(params, "optionId")?;
        if !body["options"]
            .as_array()
            .is_some_and(|opts| opts.iter().any(|o| o["id"] == option))
        {
            return Err(Error::Invalid("Option was not offered".into()));
        }
        if s.agent == "codex" {
            json!({"decision":option})
        } else if option == "allow" {
            json!({"behavior":"allow","updatedInput":body["input"]})
        } else {
            json!({"behavior":"deny","message":"Denied by the user"})
        }
    };
    Ok(if s.agent == "codex" {
        json!({"id":body["request"],"result":response})
    } else {
        json!({"type":"control_response","response":{"subtype":"success","request_id":body["request"],"response":response}})
    })
}
