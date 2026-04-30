import sys

with open('src/sync/mod.rs', 'r') as f:
    lines = f.readlines()

new_lines = []
for line in lines:
    if 'http_client: Client,' in line and 'pub struct NexusSync' not in line:
        continue # Remove field initialization in new()
    if 'http_client: Client,' in line and 'pub struct NexusSync' in line:
        continue # Skip field in struct
    if 'async fn run_websocket_listener(tx: &mpsc::Sender<StacksEvent>' in line:
        new_lines.append(line.replace('tx:', '_tx:'))
    else:
        new_lines.append(line)

with open('src/sync/mod.rs', 'w') as f:
    f.writelines(new_lines)
