#![allow(dead_code)]

use std::{collections::BTreeMap, path::Path};

use axum::{
    Router,
    body::{Body, to_bytes},
    http::{Request, StatusCode},
};
use mock_core::{
    dataset::Dataset,
    grading::{Grade, GradeInput, grade},
    serve::service_app,
    service::MockService,
    world::{SharedWorld, WorldState, shared},
};
use scenario::Scenario;
use serde_json::Value;
use tower::ServiceExt;

pub struct ContractTrial {
    pub scenario: Scenario,
    pub world: SharedWorld,
    apps: BTreeMap<&'static str, ServiceApp>,
    service: &'static str,
}

pub type ServiceSpec<'a> = (&'a dyn MockService, &'a [(&'a str, &'a str)]);

struct ServiceApp {
    app: Router,
    host: String,
    headers: Vec<(String, String)>,
}

impl ContractTrial {
    pub fn load_with_dataset(
        id: &str,
        service: &dyn MockService,
        headers: &[(&str, &str)],
        prepare: impl FnOnce(&mut Dataset),
    ) -> Self {
        Self::load_many_with_dataset(id, &[(service, headers)], prepare)
    }

    pub fn load_many_with_dataset(
        id: &str,
        services: &[ServiceSpec<'_>],
        prepare: impl FnOnce(&mut Dataset),
    ) -> Self {
        let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../scenarios")
            .join(id);
        let scenario = Scenario::load(&directory).unwrap();
        let mut dataset = Dataset::load(&directory).unwrap();
        prepare(&mut dataset);
        Self::from_loaded_source(scenario, dataset, services)
    }

    pub fn load_from_directory(directory: &Path, services: &[ServiceSpec<'_>]) -> Self {
        let scenario = Scenario::load(directory).unwrap();
        let dataset = Dataset::load(directory).unwrap();
        Self::from_loaded_source(scenario, dataset, services)
    }

    fn from_loaded_source(
        scenario: Scenario,
        dataset: Dataset,
        services: &[ServiceSpec<'_>],
    ) -> Self {
        let world = shared(WorldState::from_dataset(dataset).unwrap());
        let service = services.first().expect("a trial needs a service").0.name();
        let apps = services
            .iter()
            .map(|(service, headers)| {
                (
                    service.name(),
                    ServiceApp {
                        app: service_app(world.clone(), *service),
                        host: format!("world:{}", service.port()),
                        headers: headers
                            .iter()
                            .map(|(name, value)| (name.to_string(), value.to_string()))
                            .collect(),
                    },
                )
            })
            .collect();
        Self {
            scenario,
            world,
            apps,
            service,
        }
    }

    pub async fn request(&self, method: &str, path: &str, body: Value) -> (StatusCode, Value) {
        self.request_from(self.service, method, path, body).await
    }

    pub async fn request_from(
        &self,
        service: &str,
        method: &str,
        path: &str,
        body: Value,
    ) -> (StatusCode, Value) {
        let service = self.apps.get(service).expect("unconfigured trial service");
        let mut request = Request::builder()
            .method(method)
            .uri(path)
            .header("host", &service.host)
            .header("content-type", "application/json");
        for (name, value) in &service.headers {
            request = request.header(name.as_str(), value.as_str());
        }
        let request = request
            .body(Body::from(if body.is_null() {
                String::new()
            } else {
                body.to_string()
            }))
            .unwrap();
        let response = service.app.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let bytes = to_bytes(response.into_body(), usize::MAX).await.unwrap();
        let result = if bytes.is_empty() {
            Value::Null
        } else {
            serde_json::from_slice(&bytes).unwrap()
        };
        (status, result)
    }

    pub async fn call(&self, method: &str, path: &str, body: Value) -> Value {
        self.call_from(self.service, method, path, body).await
    }

    pub async fn call_from(&self, service: &str, method: &str, path: &str, body: Value) -> Value {
        let (status, result) = if service == self.service {
            self.request(method, path, body).await
        } else {
            self.request_from(service, method, path, body).await
        };
        assert!(status.is_success(), "{method} {path}: {status} {result}");
        assert!(result.get("errors").is_none(), "{result}");
        assert_ne!(result["isError"], true, "{result}");
        result
    }

    pub fn state(&self) -> Value {
        self.world.lock().unwrap().snapshot()["services"][self.service].clone()
    }

    pub fn assess(&self) -> Grade {
        self.assess_with_answer(None)
    }

    pub fn assess_with_answer(&self, answer: Option<&str>) -> Grade {
        let world = self.world.lock().unwrap();
        grade(&GradeInput {
            expect: &self.scenario.expect,
            state: world.snapshot(),
            journal: world.journal.entries(),
            answer,
            tools_ips: vec!["unknown".into()],
            context_overflow: false,
            startup: &[],
            initial_state: Some(&world.initial_snapshot()),
            events: Some(world.events().entries()),
        })
    }
}
