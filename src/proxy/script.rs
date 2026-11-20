use rquickjs::{Context, Function, Runtime};
use serde_json::{Value, json};
use std::sync::mpsc::Sender;
use std::time::{Duration, Instant};

pub struct Ctx {
    pub method: String,
    pub url: String,
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: String,
}

impl Ctx {
    fn to_json(&self) -> Value {
        let h: serde_json::Map<String, Value> = self.headers.iter().map(|(k, v)| (k.clone(), json!(v))).collect();
        json!({"method": self.method, "url": self.url, "status": self.status, "headers": h, "body": self.body})
    }

    fn apply(&mut self, v: &Value) {
        if let Some(s) = v.get("method").and_then(|x| x.as_str()) {
            self.method = s.into();
        }
        if let Some(s) = v.get("url").and_then(|x| x.as_str()) {
            self.url = s.into();
        }
        if let Some(s) = v.get("status").and_then(|x| x.as_u64()) {
            self.status = s as u16;
        }
        if let Some(h) = v.get("headers").and_then(|x| x.as_object()) {
            self.headers = h.iter().filter_map(|(k, v)| v.as_str().map(|v| (k.clone(), v.to_string()))).collect();
        }
        match v.get("body") {
            Some(Value::String(s)) => self.body = s.clone(),
            Some(Value::Null) | None => {}
            Some(other) => self.body = other.to_string(),
        }
    }
}

// Runs `onRequest` / `onResponse` from user code against ctx. Returns an error string on failure.
pub fn run(code: &str, hook: &str, ctx: &mut Ctx, log: &Sender<String>) -> Result<(), String> {
    let rt = Runtime::new().map_err(|e| e.to_string())?;
    rt.set_memory_limit(64 << 20);
    let deadline = Instant::now() + Duration::from_secs(2);
    rt.set_interrupt_handler(Some(Box::new(move || Instant::now() > deadline)));
    let js = Context::full(&rt).map_err(|e| e.to_string())?;
    let input = ctx.to_json().to_string();
    let out: Result<String, String> = js.with(|c| {
        let tx = log.clone();
        let f = Function::new(c.clone(), move |m: String| {
            let _ = tx.send(m);
        })
        .map_err(|e| e.to_string())?;
        c.globals().set("__log", f).map_err(|e| e.to_string())?;
        let prog = format!(
            r#"(function(){{
var console={{log:function(){{__log(Array.prototype.map.call(arguments,function(a){{return typeof a==='string'?a:JSON.stringify(a)}}).join(' '))}}}};
console.info=console.warn=console.error=console.log;
{code}
;var ctx=JSON.parse({input_lit});
if(typeof {hook}==='function'){{var r={hook}(ctx);if(r!==undefined&&r!==null)ctx=r;}}
return JSON.stringify(ctx);}})()"#,
            input_lit = serde_json::to_string(&input).unwrap(),
        );
        c.eval::<String, _>(prog).map_err(|e| match e {
            rquickjs::Error::Exception => {
                let ex = c.catch();
                format!("{:?}", ex.as_exception().map(|e| e.message().unwrap_or_default()).unwrap_or_default())
            }
            e => e.to_string(),
        })
    });
    let v: Value = serde_json::from_str(&out?).map_err(|e| e.to_string())?;
    ctx.apply(&v);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hooks_mutate_ctx() {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut c = Ctx { method: "GET".into(), url: "http://a/".into(), status: 200, headers: vec![], body: "{}".into() };
        let code = "function onRequest(c){ c.headers['x']='1'; console.log('hi', c.method); c.body = JSON.stringify({a:1}); return c; }";
        run(code, "onRequest", &mut c, &tx).unwrap();
        assert_eq!(c.headers, vec![("x".to_string(), "1".to_string())]);
        assert_eq!(c.body, r#"{"a":1}"#);
        assert_eq!(rx.try_recv().unwrap(), "hi GET");
        assert!(run("while(true){}; function onRequest(){}", "onRequest", &mut c, &tx).is_err());
        assert!(run("function onRequest(){ throw new Error('boom') }", "onRequest", &mut c, &tx).unwrap_err().contains("boom"));
    }
}
