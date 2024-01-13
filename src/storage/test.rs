// This macro is used to generate tests for each storage type.
// The first argument is the test name. The second argument is a struct type
// that contains field storage that implements the Storage trait.
#[allow(unused_macros)]
macro_rules! storage_tests {
    ($type: ty) => {
        use futures::stream::TryStreamExt;
        use tokio::io::{AsyncReadExt, AsyncWriteExt};

        type TestResult = std::result::Result<(), Box<dyn std::error::Error>>;

        #[tokio::test]
        async fn write_read_returns_content_back() -> TestResult {
            let state = <$type>::new().await;

            let mut file_1 = state.storage.write(Collection::Snapshot, "key_1").await?;
            file_1.write("Hello World!".as_bytes()).await?;

            match state.storage.read(Collection::Snapshot, "key_1").await {
                Ok(_) => {
                    panic!("Expected item to not exist yet, got Ok");
                }
                Err(err) => {
                    assert_eq!(err.kind(), io::ErrorKind::NotFound);
                }
            }
            file_1.finish().await?;

            let mut content = String::new();
            state
                .storage
                .read(Collection::Snapshot, "key_1")
                .await?
                .read_to_string(&mut content)
                .await?;
            assert_eq!(content, "Hello World!");

            Ok(())
        }

        #[tokio::test]
        async fn get_collection_items_returns_correct_files() -> TestResult {
            let state = <$type>::new().await;

            _ = state.storage.write(Collection::Snapshot, "1_key").await?.finish().await?;
            _ = state.storage.write(Collection::Snapshot, "2_key").await?.finish().await?;
            _ = state.storage.write(Collection::Snapshot, "key_3").await?.finish().await?;
            _ = state.storage.write(Collection::Snapshot, "key_4").await?.finish().await?;
            _ = state.storage.write(Collection::Blob, "key_5").await?.finish().await?;

            let mut snapshot_collection = state
                .storage
                .get_collection_items(Collection::Snapshot)
                .await?
                .try_collect::<Vec<_>>()
                .await?;
            let mut blob_collection = state
                .storage
                .get_collection_items(Collection::Blob)
                .await?
                .try_collect::<Vec<_>>()
                .await?;

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
                .await?
                .try_collect::<Vec<_>>()
                .await?;
            assert_eq!(snapshot_collection, Vec::<String>::new());

            Ok(())
        }

        #[tokio::test]
        async fn read_unknown_returns_not_found() -> TestResult {
            let state = <$type>::new().await;

            _ = state.storage.write(Collection::Snapshot, "key_1").await?.finish().await?;

            let res = state.storage.read(Collection::Snapshot, "key_2").await;
            match res {
                Ok(_) => panic!("Expected error"),
                Err(err) => assert_eq!(err.kind(), io::ErrorKind::NotFound),
            }

            Ok(())
        }
    };
}
