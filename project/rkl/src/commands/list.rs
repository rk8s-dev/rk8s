use crate::cri::cri_api::{self, ContainerFilter};
use anyhow::{Ok, Result};
use liboci_cli::List;

use crate::cri::cri_api::ListContainersRequest;

pub fn list(args: List) -> Result<()> {
    let request = ListContainersRequest {
        filter: Some(ContainerFilter {
            id: todo!(),
            state: todo!(),
            pod_sandbox_id: todo!(),
            label_selector: todo!(),
        }),
    };

    Ok(())
}
