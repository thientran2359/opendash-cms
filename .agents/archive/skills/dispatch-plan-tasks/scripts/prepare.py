"""Prepare a task worktree without touching the main checkout. No dispatch."""
import base64
import json
import re
import subprocess
import sys
from pathlib import Path


def git(repo, *args):
    return subprocess.check_output(['git', '-C', str(repo), *args], text=True).strip()


def prepare(d):
    required = ('repo worktree origin baseline task_id recipient plan assignment acceptance '
                'checks owned_files outside_scope prerequisites').split()
    for field in required:
        if not d.get(field):
            raise ValueError('Missing field: ' + field)
    if not re.fullmatch(r'[A-Za-z0-9][A-Za-z0-9.-]{0,79}', d['task_id']):
        raise ValueError('Invalid task ID')
    if d['recipient'] not in ('cos-win', 'cos-mac', 'ag-mac'):
        raise ValueError('Invalid recipient')
    if not re.fullmatch(r'[a-f0-9]{40}', d['baseline']):
        raise ValueError('Use full baseline SHA')
    repo, wt = Path(d['repo']).resolve(), Path(d['worktree']).resolve()
    if wt == repo or repo in wt.parents or wt.exists():
        raise ValueError('Worktree must be new and outside main checkout')
    if git(repo, 'remote', 'get-url', 'origin') != d['origin']:
        raise ValueError('Origin mismatch')
    if git(repo, 'remote', 'get-url', '--push', 'origin') != d['origin']:
        raise ValueError('Push origin mismatch')
    git(repo, 'cat-file', '-e', d['baseline'] + '^{commit}')
    plan = d['plan'].split('#')[0]
    if Path(plan).is_absolute() or Path(plan).root or '\\' in plan or '..' in Path(plan).parts:
        raise ValueError('Plan must be a repo-relative POSIX path')
    git(repo, 'cat-file', '-e', d['baseline'] + ':' + plan)
    if not isinstance(d['owned_files'], list) or not all(isinstance(x, str) and x.strip() for x in d['owned_files']):
        raise ValueError('owned_files must be a list of paths')
    branch = d['recipient'] + '/' + d['task_id'].lower()
    git(repo, 'check-ref-format', '--branch', branch)
    # Fail on existing local or remote-tracking branch: never adopt or reset previous work.
    for ref in ('refs/heads/' + branch, 'refs/remotes/origin/' + branch):
        if subprocess.run(['git', '-C', str(repo), 'show-ref', '--verify', '--quiet', ref]).returncode == 0:
            raise ValueError('Branch already exists: ' + ref)
    wt.parent.mkdir(parents=True, exist_ok=True)
    git(repo, 'worktree', 'add', '-b', branch, str(wt), d['baseline'])
    sync = wt / '.agents/sync'
    sync.mkdir(parents=True, exist_ok=True)
    task = sync / 'task-for-ag.md'
    if subprocess.run(['git', '-C', str(wt), 'check-ignore', '-q', str(task)]).returncode != 0:
        raise ValueError('Task path is not ignored; worktree retained, no dispatch')
    report = 'docs/verification/' + d['task_id'].lower() + '-' + d['recipient'] + '.md'
    text = f"# {d['task_id']}\n\nWorkspace: {wt}\nBranch: {branch}\nBaseline: {d['baseline']}\nOrigin: {d['origin']}\nPlan: {d['plan']}\n"
    for key in ('assignment', 'prerequisites', 'owned_files', 'outside_scope', 'acceptance', 'checks'):
        value = d[key]
        text += '\n## ' + key + '\n' + ('\n'.join('- ' + x for x in value) if isinstance(value, list) else str(value)) + '\n'
    text += f'''
## Execution and handoff
First verify pwd, branch, baseline and origin above; stop on mismatch. Read AGENTS.md
and referenced contracts. Implement only this task in this worktree. Reuse required
core APIs. Stop-and-Propose when design/gates/contracts need change; do not redesign.
No additional workers, deployment, main push, merge, force push or shared plan status edits.
Write {report}: task ID, baseline/branch, changes, exact checks/results, limitations,
blockers and Design Delta. Do not claim unrun checks passed. Run `python cms/scripts/check-plans.py`
and `git diff --check` before committing. Keep handoff to the user.
This task explicitly authorizes scoped commit and push of owned task files and {report} on {branch}; push with:
git push -u origin {branch}
Verify push succeeded and give final commit + branch + report path. If blocked or push
fails, report that fact without claiming completion. Never expose credentials.
'''
    task.write_text(text, encoding='utf-8')
    receipt = {'task_id': d['task_id'], 'recipient': d['recipient'], 'baseline': d['baseline'],
               'branch': branch, 'worktree': str(wt), 'task_file': str(task), 'delivery': 'PREPARED'}
    (sync / 'dispatch-prepared.json').write_text(json.dumps(receipt, indent=2), encoding='utf-8')
    return receipt


if __name__ == '__main__':
    arg = sys.argv[1]
    data = json.loads(base64.b64decode(arg)) if not arg.endswith('.json') else json.loads(Path(arg).read_text(encoding='utf-8-sig'))
    print(json.dumps(prepare(data)))
