use apis::client::APIClient;
use models;
use models::{ServiceBindingRequest, ServiceInstanceProvisionRequest};
use prettytable::{format, Table};
use serde_json::json;
use spinners::{Spinner, Spinners};
use std::error::Error;
use std::{thread, time};
use uuid::Uuid;

const USER_AGENT: &str = "ROCS v0.1";
const DEFAULT_API_VERSION: &str = "2.15";

pub struct Options {
    pub json_output: bool,
    pub synchronous: bool,
}

pub fn catalog(
    _: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let catalog_api = client.catalog_api();
    let catalog = catalog_api
        .catalog_get(DEFAULT_API_VERSION)
        .expect("catalog request failed");

    match options.json_output {
        false => {
            let mut services_table = Table::new();

            services_table.add_row(row!["Service", "Description", "Plans", "Extensions"]);

            for s in catalog.services.unwrap().iter() {
                let mut plans_table = Table::new();
                let mut extensions_table = Table::new();

                plans_table.set_format(*format::consts::FORMAT_NO_BORDER_LINE_SEPARATOR);

                for p in s.plans.iter() {
                    plans_table.add_row(row![p.name]);
                }

                if let Some(extensions) = &s.extensions {
                    for p in extensions.iter() {
                        extensions_table.add_row(row![p.id]);
                    }
                }

                services_table.add_row(row![s.name, s.description, plans_table]);
            }

            services_table.printstd();
        }
        true => {
            println!(
                "{}",
                serde_json::to_string(&catalog.services.unwrap()).unwrap()
            );
        }
    };

    Ok(())
}

pub fn deprovision(
    matches: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let instance_id = matches.value_of("instance-id").unwrap().to_string();
    let si_api = client.service_instances_api();
    si_api
        .service_instance_deprovision(
            DEFAULT_API_VERSION,
            &*instance_id,
            "",
            "",
            USER_AGENT,
            !options.synchronous,
        )
        .expect("deprovisioning request failed");
    Ok(())
}

pub fn provision(
    matches: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let service = matches.value_of("service").unwrap().to_string();
    let plan = matches.value_of("plan").unwrap().to_string();

    let si_api = client.service_instances_api();

    let (service_id, plan_id) =
        find_service_plan_id(&client, service, plan).expect("service or plan id not found");

    let mut provision_request = ServiceInstanceProvisionRequest::new(
        service_id.clone(),
        plan_id.clone(),
        String::from(""),
        String::from(""),
    );

    // TODO(rsampaio): receive parameters from caller
    provision_request.parameters = Some(json!({"region": "us-east-1"}));

    let instance_id = Uuid::new_v4().to_hyphenated().to_string();

    let _provision_response = si_api
        .service_instance_provision(
            DEFAULT_API_VERSION,
            &*instance_id, // from String to &str
            provision_request,
            USER_AGENT,
            !options.synchronous, // Accepts-incomplete
        )
        .expect("provision request failed");

    if matches.is_present("wait") {
        let sp = Spinner::new(Spinners::Point, "provisioning service instance".to_string());

        loop {
            let last_op = si_api.service_instance_last_operation_get(
                DEFAULT_API_VERSION,
                &*instance_id,
                &*service_id, // service_id
                &*plan_id,    // plan id
                "",           // operation
            );

            if let Ok(lo) = last_op {
                match lo.state {
                    models::State::InProgress => {
                        thread::sleep(time::Duration::new(2, 0));
                    }
                    _ => break,
                }
            }
        }
        println!("");
        sp.stop();
    }

    let provisioned_instance = si_api
        .service_instance_get(DEFAULT_API_VERSION, &*instance_id, USER_AGENT, "", "")
        .expect("service instance fetch failed");

    match options.json_output {
        false => {
            let mut table = Table::new();
            table.add_row(row!["Instance ID", "Dashboard URL"]);
            table.add_row(row![
                &*instance_id,
                provisioned_instance.dashboard_url.unwrap()
            ]);
            table.printstd();
        }
        true => {
            println!("{}", serde_json::to_string(&provisioned_instance).unwrap());
        }
    };

    Ok(())
}

pub fn bind(
    matches: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let binding_api = client.service_bindings_api();

    let binding_id = Uuid::new_v4().to_hyphenated().to_string();
    let instance_id = matches.value_of("instance-id").unwrap().to_string();

    let binding_request = ServiceBindingRequest::new("".into(), "".into());
    let _binding_response = binding_api.service_binding_binding(
        DEFAULT_API_VERSION,
        &*instance_id,
        &*binding_id,
        binding_request,
        USER_AGENT,
        !options.synchronous,
    );

    if !options.synchronous {
        let sp = Spinner::new(Spinners::Point, "provisioning binding".to_string());

        loop {
            let last_op = binding_api.service_binding_last_operation_get(
                DEFAULT_API_VERSION,
                &*instance_id,
                &*binding_id,
                "", // service_id
                "", // plan id
                "", // operation
            );

            if let Ok(lo) = last_op {
                match lo.state {
                    models::State::InProgress => {
                        thread::sleep(time::Duration::new(2, 0));
                    }
                    _ => break,
                }
            }
        }
        println!("");
        sp.stop();
    }

    let provisioned_binding = binding_api
        .service_binding_get(
            DEFAULT_API_VERSION,
            &*instance_id,
            &*binding_id,
            USER_AGENT,
            "",
            "",
        )
        .expect("service binding fetch failed");

    match options.json_output {
        false => {
            let mut table = Table::new();
            table.add_row(row!["Instance ID"]);
            table.add_row(row![&*instance_id]);
            table.add_row(row!["Binding ID"]);
            table.add_row(row![&*binding_id]);
            table.add_row(row!["Credentials"]);
            table.add_row(row![serde_json::to_string_pretty(&provisioned_binding)?]);
            table.printstd();
        }
        true => println!("{}", serde_json::to_string(&provisioned_binding).unwrap()),
    };

    Ok(())
}

pub fn unbind(
    matches: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let binding_api = client.service_bindings_api();

    let instance_id = matches.value_of("instance-id").unwrap().to_string();
    let binding_id = matches.value_of("binding-id").unwrap().to_string();

    let _unbinding_response = binding_api.service_binding_unbinding(
        DEFAULT_API_VERSION,
        &*instance_id,
        &*binding_id,
        "", // service_id
        "", // plan_id
        USER_AGENT,
        !options.synchronous,
    ).expect("service binding unbind failed");

    Ok(())
}

pub fn creds(
    matches: &clap::ArgMatches,
    client: APIClient,
    options: Options,
) -> Result<(), Box<dyn Error>> {
    let binding_api = client.service_bindings_api();

    let instance_id = matches.value_of("instance-id").unwrap().to_string();
    let binding_id = matches.value_of("binding-id").unwrap().to_string();

    let provisioned_binding = binding_api
        .service_binding_get(
            DEFAULT_API_VERSION,
            &*instance_id,
            &*binding_id,
            USER_AGENT,
            "",
            "",
        )
        .expect("service binding fetch failed");

    match options.json_output {
        false => {
            let mut table = Table::new();
            table.add_row(row!["Instance ID"]);
            table.add_row(row![&*instance_id]);
            table.add_row(row!["Binding ID"]);
            table.add_row(row![&*binding_id]);
            table.add_row(row!["Credentials"]);
            table.add_row(row![serde_json::to_string_pretty(&provisioned_binding)?]);
            table.printstd();
        }
        true => println!("{}", serde_json::to_string(&provisioned_binding).unwrap()),
    };

    Ok(())
}

fn find_service_plan_id(
    client: &APIClient,
    service: String,
    plan: String,
) -> Result<(String, String), &'static str> {
    let si_api = client.catalog_api();
    let catalog = si_api
        .catalog_get(DEFAULT_API_VERSION)
        .expect("failed to fetch catalog");

    let mut service_id = String::from("");
    let mut plan_id = String::from("");

    'outer: for s in catalog.services.unwrap() {
        if s.name == service {
            service_id = s.id;
            for p in s.plans {
                if p.name == plan {
                    plan_id = p.id;
                    break 'outer;
                }
            }
        }
    }

    if plan_id == "" || service_id == "" {
        return Err("plan or service not found");
    }

    Ok((service_id, plan_id))
}
