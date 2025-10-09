#[cfg(test)]
mod tests {
    use json_digest::digest_data;
    use serde_json::json;
    use serde_yaml::Value as YamlValue;
    use tokio::fs::write;

    #[tokio::test]
    async fn test_canonical_hashing_consistency() {
        // Test that our hashing approach produces consistent results
        // regardless of how the YAML is structured internally

        // Create the same logical data structure in different ways
        let data1 = json!({
            "id": "test-123",
            "name": "Test Runbook",
            "version": 1,
            "content": {
                "steps": [
                    {"name": "Step 1", "command": "echo hello"},
                    {"name": "Step 2", "command": "echo world"}
                ],
                "description": "A test runbook"
            }
        });

        let data2 = json!({
            "version": 1,
            "content": {
                "description": "A test runbook",
                "steps": [
                    {"command": "echo hello", "name": "Step 1"},
                    {"command": "echo world", "name": "Step 2"}
                ]
            },
            "name": "Test Runbook",
            "id": "test-123"
        });

        // Both should produce the same canonical hash
        let hash1 = digest_data(&data1).expect("Failed to hash data1");
        let hash2 = digest_data(&data2).expect("Failed to hash data2");

        assert_eq!(
            hash1, hash2,
            "Canonical hashing should produce same hash regardless of key order"
        );
    }

    #[tokio::test]
    async fn test_yaml_to_json_conversion_preserves_hash() {
        // Test that converting YAML->JSON->canonical hash works correctly

        let original_json = json!({
            "id": "test-456",
            "name": "Another Test",
            "version": 1,
            "content": {
                "description": "Test description",
                "steps": [{"name": "Do something", "command": "ls -la"}]
            }
        });

        // Convert to YAML and back to JSON (simulating our save/load process)
        let yaml_value: YamlValue =
            serde_yaml::to_value(&original_json).expect("Failed to convert to YAML");
        let reconstructed_json: serde_json::Value =
            serde_yaml::from_value(yaml_value).expect("Failed to convert back to JSON");

        // Both should produce the same canonical hash
        let original_hash = digest_data(&original_json).expect("Failed to hash original");
        let reconstructed_hash =
            digest_data(&reconstructed_json).expect("Failed to hash reconstructed");

        assert_eq!(
            original_hash, reconstructed_hash,
            "YAML roundtrip should preserve canonical hash"
        );
    }

    #[tokio::test]
    async fn test_file_save_load_hash_consistency() {
        // Test the full save/load cycle produces consistent hashes
        let temp_dir = tempfile::tempdir().expect("Failed to create temp dir");
        let temp_path = temp_dir.path().join("test.atrb");

        let original_data = json!({
            "id": "save-load-test",
            "name": "Save Load Test",
            "version": 1,
            "content": {
                "description": "Testing save/load cycle",
                "metadata": {"author": "test", "created": "2024-01-01"}
            }
        });

        // Convert to YAML structure like workspace.rs does
        let mut map = serde_yaml::Mapping::new();
        map.insert(
            YamlValue::String("id".to_string()),
            YamlValue::String("save-load-test".to_string()),
        );
        map.insert(
            YamlValue::String("name".to_string()),
            YamlValue::String("Save Load Test".to_string()),
        );
        map.insert(
            YamlValue::String("version".to_string()),
            YamlValue::Number(1.into()),
        );
        map.insert(
            YamlValue::String("content".to_string()),
            serde_yaml::to_value(&original_data["content"]).unwrap(),
        );
        let yaml_structure = YamlValue::Mapping(map);

        // Save to file
        let yaml_text =
            serde_yaml::to_string(&yaml_structure).expect("Failed to serialize to YAML");
        write(&temp_path, yaml_text)
            .await
            .expect("Failed to write file");

        // Calculate hash like workspace.rs does (canonical via JSON)
        let json_for_hash: serde_json::Value =
            serde_yaml::from_value(yaml_structure).expect("Failed to convert to JSON for hashing");
        let save_hash = digest_data(&json_for_hash).expect("Failed to hash for save");

        // Now load the file like fs_ops.rs does
        let loaded_yaml_text = tokio::fs::read_to_string(&temp_path)
            .await
            .expect("Failed to read file");
        let loaded_yaml: YamlValue =
            serde_yaml::from_str(&loaded_yaml_text).expect("Failed to parse YAML");
        let loaded_json: serde_json::Value =
            serde_yaml::from_value(loaded_yaml).expect("Failed to convert loaded YAML to JSON");
        let load_hash = digest_data(&loaded_json).expect("Failed to hash loaded data");

        assert_eq!(
            save_hash, load_hash,
            "Save and load should produce identical canonical hashes"
        );
    }
}
