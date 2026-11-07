//! Split source modules are scoped to the accepted HAP entry's directory.
use std::sync::{Arc, Weak};

use crate::app::{
    runtime::{Dispatcher, Message},
    session::Services,
};
pub(crate) struct ResourceChunks {
    host: Arc<Services>,
    prefix: String,
}
impl ResourceChunks {
    pub(crate) fn new(host: Arc<Services>, entry: &str) -> Arc<Self> {
        let prefix = entry
            .rsplit_once('/')
            .map_or(String::new(), |(p, _)| format!("{p}/"));
        Arc::new(Self { host, prefix })
    }
    pub(crate) fn read_chunk(&self, id: &str) -> Result<nsh_engine::Chunk, String> {
        crate::app::session::resource_name(id).map_err(|e| e.to_string())?;
        if id.contains('/') || !id.ends_with(".js") {
            return Err("expected a chunk file name".into());
        }
        let name = format!("{}{id}", self.prefix);
        let bytes = self.host.resource(&name).map_err(|e| e.to_string())?;
        let code = String::from_utf8(bytes).map_err(|_| "chunk source must be UTF-8")?;
        let mut chunk = nsh_engine::Chunk::source(id.to_owned(), code);
        if let Ok(bytes) = self.host.resource(&format!("{name}.map"))
            && let Ok(map) = String::from_utf8(bytes)
            && !map.is_empty()
        {
            chunk.source_map = Some(map.into());
        }
        Ok(chunk)
    }
    pub(crate) fn collect_entry_chunks(
        &self,
        bundle: &nsh_engine::Bundle,
        manifest_specs: &[String],
    ) -> Result<Vec<nsh_engine::Chunk>, String> {
        let mut seeds: Vec<String> = manifest_specs.to_vec();
        if let nsh_engine::BundleCode::Source(ref code) = bundle.code {
            for spec in static_chunk_imports(code) {
                if !seeds.contains(&spec) {
                    seeds.push(spec);
                }
            }
        }
        let mut visited = Vec::new();
        let mut ordered = Vec::new();
        for spec in seeds {
            self.visit_entry_chunk(&spec, &mut visited, &mut ordered)?;
        }
        Ok(ordered)
    }
    fn visit_entry_chunk(
        &self,
        spec: &str,
        visited: &mut Vec<String>,
        ordered: &mut Vec<nsh_engine::Chunk>,
    ) -> Result<(), String> {
        if visited.iter().any(|item| item == spec) {
            return Ok(());
        }
        visited.push(spec.to_owned());
        let chunk = self.read_chunk(spec)?;
        if let nsh_engine::BundleCode::Source(ref code) = chunk.code {
            for dep in static_chunk_imports(code) {
                self.visit_entry_chunk(&dep, visited, ordered)?;
            }
        }
        ordered.push(chunk);
        Ok(())
    }
    pub(crate) fn load(self: &Arc<Self>, id: &str, route: Weak<Dispatcher>) -> bool {
        let id = id.to_owned();
        let loader = self.clone();
        let permit = match self.host.reserve_load() {
            Ok(p) => p,
            Err(e) => {
                if let Some(route) = route.upgrade() {
                    route.send(Message::Chunk(Err((id, e.to_string()))));
                }
                return true;
            }
        };
        let failed_route = route.clone();
        let failed_id = id.clone();
        let result = std::thread::Builder::new()
            .name("nsh-chunk".into())
            .spawn(move || {
                let _permit = permit;
                let result = loader.read_chunk(&id);
                if let Some(route) = route.upgrade() {
                    route.send(Message::Chunk(result.map_err(|e| (id, e))));
                }
            });
        if let Err(error) = result
            && let Some(route) = failed_route.upgrade()
        {
            route.send(Message::Chunk(Err((failed_id, error.to_string()))));
        }
        true
    }
}

pub(crate) fn static_chunk_imports(source: &str) -> Vec<String> {
    let mut specs = Vec::new();
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with("//") {
            if trimmed.starts_with("//#region") {
                break;
            }
            continue;
        }
        if !trimmed.starts_with("import ")
            && !trimmed.starts_with("import{")
            && !trimmed.starts_with("import*")
            && !trimmed.starts_with("import\"")
            && !trimmed.starts_with("import'")
            && !trimmed.starts_with("export ")
            && !trimmed.starts_with("export{")
            && !trimmed.starts_with("export*")
        {
            break;
        }
        if let Some(spec) = extract_relative_chunk_specifier(trimmed)
            && !specs.contains(&spec)
        {
            specs.push(spec);
        }
    }
    specs
}

fn extract_relative_chunk_specifier(stmt: &str) -> Option<String> {
    for prefix in ["\"./", "'./"] {
        let quote = prefix.chars().next()?;
        if let Some(pos) = stmt.rfind(prefix) {
            let rest = &stmt[pos + prefix.len()..];
            let end = rest.find(quote)?;
            let spec = &rest[..end];
            if !spec.is_empty() && !spec.contains('/') && spec.ends_with(".js") {
                return Some(spec.to_owned());
            }
        }
    }
    None
}
