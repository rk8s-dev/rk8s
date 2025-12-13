use common::{Deployment, RksMessage};
use std::collections::HashMap;

#[test]
fn test_deployment_serialization() {
    let yaml = r#"
apiVersion: apps/v1
kind: Deployment
metadata:
  name: nginx-deployment
  labels:
    app: nginx
spec:
  replicas: 3
  selector:
    matchLabels:
      app: nginx
  template:
    metadata:
      labels:
        app: nginx
    spec:
      containers:
      - name: nginx
        image: nginx:1.14.2
        ports:
        - containerPort: 80
"#;

    let deploy: Deployment = serde_yaml::from_str(yaml).unwrap();
    let msg = RksMessage::CreateDeployment(Box::new(deploy));

    let encoded = bincode::serialize(&msg).unwrap();
    println!("Encoded length: {}", encoded.len());

    let decoded: RksMessage = bincode::deserialize(&encoded).unwrap();
    if let RksMessage::CreateDeployment(d) = decoded {
        assert_eq!(d.metadata.name, "nginx-deployment");
    } else {
        panic!("Wrong variant");
    }
}
