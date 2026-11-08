"""One-shot CoS admission; never monitors completion or retries mutations."""
import argparse
import json
import uuid
import urllib.error
import urllib.request
from pathlib import Path


def send(data_dir, worktree, project_id):
    if str(uuid.UUID(project_id)) != project_id:
        raise ValueError('Use a lowercase canonical project UUID')
    wt = Path(worktree).resolve()
    sync = wt / '.agents/sync'
    prepared = json.loads((sync / 'dispatch-prepared.json').read_text(encoding='utf-8'))
    if Path(prepared['worktree']).resolve() != wt or prepared['recipient'] not in ('cos-win', 'cos-mac'):
        raise ValueError('Wrong prepared worktree/recipient')
    receipt = sync / 'dispatch-send.json'
    api = Path(data_dir) / 'control-api'
    endpoint = json.loads((api / 'endpoint.json').read_text(encoding='utf-8'))
    token = (api / 'token').read_text(encoding='utf-8').strip()
    port = int(endpoint['port'])
    if not 1 <= port <= 65535:
        raise ValueError('Invalid local API port')
    base = f'http://127.0.0.1:{port}'
    headers = {'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'}
    with urllib.request.urlopen(urllib.request.Request(base + '/v1/health', headers=headers), timeout=15) as r:
        health = json.load(r)
    if not health.get('actions', {}).get('enabled'):
        raise ValueError('Local actions are disabled')
    task_path = wt / '.agents/sync/task-for-ag.md'
    if not task_path.is_file():
        raise ValueError('Missing task file')
    input_id = str(uuid.uuid4())
    body = {'id': input_id, 'projectId': project_id,
            'text': f'Task {prepared["task_id"]}: Read and execute {task_path}. Work ONLY in {wt} on branch {prepared["branch"]}. '
                    'Verify the workspace before edits; stop on mismatch. Task file grants scoped commit/push. '
                    'Do not spawn workers. Report directly in this chat to the user; coordinator will not monitor.'}
    state = {'delivery': 'UNKNOWN', 'input_id': input_id, 'session_id': input_id, 'project_id': project_id,
             'task_id': prepared['task_id']}
    # Exclusive attempt record survives network ambiguity; subsequent sends fail closed.
    with receipt.open('x', encoding='utf-8') as f:
        json.dump(state, f, indent=2)
    request = urllib.request.Request(base + '/v1/inputs',
                                     data=json.dumps(body).encode(), headers=headers)
    try:
        with urllib.request.urlopen(request, timeout=30) as r:
            response = json.load(r)
            state.update(delivery='ACCEPTED' if r.status == 202 else 'UNKNOWN', response=response)
    except urllib.error.HTTPError as exc:
        # Record only status code; never include headers/token/error response payloads.
        state.update(
            delivery='REJECTED' if 400 <= exc.code < 500 else 'UNKNOWN',
            error='HTTPError',
            http_status=exc.code,
        )
    except Exception as exc:
        # Never include headers/token/error response payloads in logs.
        state['error'] = type(exc).__name__
    receipt.write_text(json.dumps(state, indent=2), encoding='utf-8')
    return state


if __name__ == '__main__':
    p = argparse.ArgumentParser()
    for name in ('data-dir', 'worktree', 'project-id'):
        p.add_argument('--' + name, required=True)
    a = p.parse_args()
    result = send(a.data_dir, a.worktree, a.project_id)
    print(json.dumps(result))
    raise SystemExit(0 if result['delivery'] == 'ACCEPTED' else 2)
