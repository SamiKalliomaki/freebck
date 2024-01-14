// This macro is used to generate tests for each storage type.
// The first argument is the test name. The second argument is a struct type
// that contains field storage that implements the Storage trait.
#[allow(unused_macros)]
macro_rules! storage_tests {
    ($type: ty) => {
        type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

        #[tokio::test]
        async fn write_read_returns_content_back() -> TestResult {
            let state = <$type>::new().await;

            state
                .storage
                .write(Collection::Snapshot, "key_1", b"Hello World!")
                .await?;

            let mut buffer = Vec::new();
            state
                .storage
                .read(Collection::Snapshot, "key_1", &mut buffer)
                .await?;
            assert_eq!(buffer, b"Hello World!");

            Ok(())
        }

        #[tokio::test]
        async fn get_collection_items_returns_correct_files() -> TestResult {
            let state = <$type>::new().await;

            _ = state
                .storage
                .write(Collection::Snapshot, "1_key", b"")
                .await?;
            _ = state
                .storage
                .write(Collection::Snapshot, "2_key", b"")
                .await?;
            _ = state
                .storage
                .write(Collection::Snapshot, "key_3", b"")
                .await?;
            _ = state
                .storage
                .write(Collection::Snapshot, "key_4", b"")
                .await?;
            _ = state.storage.write(Collection::Blob, "key_5", b"").await?;

            let mut snapshot_collection = state
                .storage
                .get_collection_items(Collection::Snapshot)
                .await?;
            let mut blob_collection = state.storage.get_collection_items(Collection::Blob).await?;

            snapshot_collection.sort();
            blob_collection.sort();

            assert_eq!(snapshot_collection, ["1_key", "2_key", "key_3", "key_4"]);
            assert_eq!(blob_collection, ["key_5"]);

            Ok(())
        }

        #[tokio::test]
        async fn get_collection_items_returns_empty_list() -> TestResult {
            let state = <$type>::new().await;

            let snapshot_collection = state
                .storage
                .get_collection_items(Collection::Snapshot)
                .await?;
            assert_eq!(snapshot_collection, Vec::<String>::new());

            Ok(())
        }

        #[tokio::test]
        async fn read_unknown_returns_not_found() -> TestResult {
            let state = <$type>::new().await;

            _ = state
                .storage
                .write(Collection::Snapshot, "key_1", b"")
                .await?;

            let mut buffer = Vec::new();
            let res = state
                .storage
                .read(Collection::Snapshot, "key_2", &mut buffer)
                .await;
            match res {
                Ok(_) => panic!("Expected not found"),
                Err(err) => assert_eq!(err.kind(), io::ErrorKind::NotFound),
            }

            Ok(())
        }
    };
}
