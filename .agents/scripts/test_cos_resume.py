"""Resume generations must not bypass ambiguous delivery or live-task gates."""
import copy
import json
import tempfile
import unittest
import uuid
from pathlib import Path
from unittest import mock
import cos_dispatch as a


class ResumeTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory(); self.addCleanup(self.tmp.cleanup)
        self.root = Path(self.tmp.name)
        self.project = str(uuid.uuid4()); self.parent = str(uuid.uuid4())
        self.input = str(uuid.uuid4()); self.request = str(uuid.uuid4())
        for name, value in [('storage_root', self.root), ('security_check', (True, 'OK')),
                            ('load_config', {'project_id': self.project}),
                            ('probe', {'can_send': True})]:
            p = mock.patch.object(a, name, return_value=value);p.start();self.addCleanup(p.stop)
        self.args = ('harbor', self.request, 378, 'M6-S11-T01', 'a'*40, 'b'*40, 406, 'c'*40, 123)
        self.record = dict(target='harbor', request_id=self.parent, input_id=self.input,
                           mode='issue', issue=378, task_id='M6-S11-T01', source_sha='a'*40,
                           base_sha='b'*40, outcome='ACCEPTED', outbox_delivery='sent')
        a.issue_lock_file('harbor',378,'M6-S11-T01').parent.mkdir(parents=True,exist_ok=True)
        a.atomic_private_json(a.issue_lock_file('harbor',378,'M6-S11-T01'), self.record)
        a.atomic_private_json(a.receipt_file('harbor',self.parent), self.record)
        self.issue = dict(state='open',body='M6-S11-T01 '+'a'*40,
                          labels=[{'name':'agent:harbor'}])
        self.pr = dict(number=406,state='open',merged=False,body='Agent: Harbor\nIssue: #378\nM6-S11-T01',
                       head={'sha':'c'*40,'ref':'agent/harbor','repo':{'full_name':a.REPOSITORY}},
                       base={'sha':'b'*40,'ref':'main','repo':{'full_name':a.REPOSITORY}})
        self.marker = dict(repository=a.REPOSITORY,target='harbor',issue=378,task_id='M6-S11-T01',
                           source_sha='a'*40,base_sha='b'*40,pr=406,head_sha='c'*40,
                           intent='RESUME_CURRENT',coordinator='Aster')
        self.ack = dict(id=123,user={'login':'thientran2359'},
                        issue_url='https://api.github.com/repos/'+a.REPOSITORY+'/issues/378',
                        created_at='2026-11-19T16:00:02Z',body=self.body())
        self.sessions = {'sessions':[dict(id=self.input,projectId=self.project,activeTurnId=None,
                           lastTurnOutcome='completed',lastTurnEndAt=1795104000000)],'nextCursor':None}
        self.posts=[]
        p=mock.patch.object(a,'github_json',side_effect=self.github);p.start();self.addCleanup(p.stop)
        p=mock.patch.object(a,'call_api',side_effect=self.api);p.start();self.addCleanup(p.stop)

    def body(self):
        return 'Agent: Aster\n<!-- opendash-resume-v1\n'+json.dumps(self.marker)+'\n-->'

    def github(self,path):
        if '/issues/comments/' in path:return copy.deepcopy(self.ack)
        if '/pulls/' in path:return copy.deepcopy(self.pr)
        return copy.deepcopy(self.issue)

    def api(self,method,path,payload=None):
        if method=='POST':self.posts.append(payload);return 202,{}
        if path.startswith('/v1/sessions?'):return 200,copy.deepcopy(self.sessions)
        if path.startswith('/v1/inputs?'):return 200,{'inputs':[{'id':self.input,'delivery':'sent'}]}
        return 200,{'events':[{'kind':'turn_end'}]}

    def test_send_once_and_same_request_replay(self):
        r=a.resume_issue(*self.args);self.assertEqual(r['outcome'],'ACCEPTED')
        self.assertTrue(a.resume_issue(*self.args)['duplicate']);self.assertEqual(len(self.posts),1)
        self.assertIn('123', self.posts[0]['text']);self.assertNotIn(self.ack['body'],self.posts[0]['text'])
        with self.assertRaises(a.Blocked):a.resume_issue(self.args[0],str(uuid.uuid4()),*self.args[2:])
        self.assertEqual(len(self.posts),1)

    def test_closed_wrong_owner_stale_head_wrong_issue_or_ack_rejected(self):
        variants=[('issue','state','closed'),('pr','head',dict(self.pr['head'],sha='d'*40)),
                  ('ack','user',{'login':'outsider'}),('ack','issue_url','wrong'),
                  ('ack','created_at','2026-11-19T15:59:59Z'),('ack','body','Agent: Aster ACK')]
        for obj,key,value in variants:
            with self.subTest(obj=obj,key=key):
                original=copy.deepcopy(getattr(self,obj));getattr(self,obj)[key]=value
                with self.assertRaises(a.Blocked):a.resume_issue(*self.args)
                setattr(self,obj,original)
        self.assertFalse(self.posts)

    def test_active_unknown_or_unconfirmed_previous_turn_rejected(self):
        for patch in [dict(activeTurnId='running'),dict(lastTurnOutcome='failed'),dict(lastTurnEndAt=None)]:
            original=copy.deepcopy(self.sessions);self.sessions['sessions'][0].update(patch)
            with self.assertRaises(a.Blocked):a.resume_issue(*self.args)
            self.sessions=original
        self.record['outcome']='UNKNOWN'
        a.atomic_private_json(a.receipt_file('harbor',self.parent),self.record)
        with self.assertRaises(a.Blocked):a.resume_issue(*self.args)
        self.assertFalse(self.posts)

    def test_timeout_and_crash_reservation_cannot_be_retried_with_new_id(self):
        original=self.api
        def timeout(method,path,payload=None):
            if method=='POST':self.posts.append(payload);raise a.Blocked('TIMEOUT')
            return original(method,path,payload)
        with mock.patch.object(a,'call_api',side_effect=timeout):
            self.assertEqual(a.resume_issue(*self.args)['outcome'],'UNKNOWN')
        a.receipt_file('harbor',self.request).unlink()
        with self.assertRaises(a.Blocked):a.resume_issue(self.args[0],str(uuid.uuid4()),*self.args[2:])
        self.assertEqual(len(self.posts),1)

    def test_full_pagination_blocks_active_turn_on_later_page(self):
        original=self.api
        def paged(method,path,payload=None):
            if 'cursor=next' in path:return 200,{'sessions':[dict(self.sessions['sessions'][0],id='other',activeTurnId='busy')],'nextCursor':None}
            if path.startswith('/v1/sessions?'):return 200,dict(self.sessions,nextCursor='next')
            return original(method,path,payload)
        with mock.patch.object(a,'call_api',side_effect=paged):
            with self.assertRaises(a.Blocked):a.resume_issue(*self.args)
        self.assertFalse(self.posts)

    def test_head_race_after_reservation_never_posts(self):
        original=self.github;n=0
        def race(path):
            nonlocal n
            r=original(path)
            if '/pulls/' in path:
                n+=1
                if n>1:r['head']['sha']='d'*40
            return r
        with mock.patch.object(a,'github_json',side_effect=race):
            self.assertEqual(a.resume_issue(*self.args)['outcome'],'UNKNOWN')
        self.assertFalse(self.posts)

    def test_host_failure_does_not_read_github(self):
        with mock.patch.object(a,'security_check',return_value=(False,'DENIED')),mock.patch.object(a,'github_json') as gh:
            with self.assertRaises(a.Blocked):a.resume_issue(*self.args)
            gh.assert_not_called()

    def test_two_generations_and_status_follow_latest(self):
        first=a.resume_issue(*self.args)
        self.input=first['input_id']
        self.sessions['sessions'][0].update(id=self.input,lastTurnEndAt=1795104003000)
        self.ack.update(id=124,created_at='2026-11-19T16:00:04Z')
        second_args=list(self.args);second_args[1]=str(uuid.uuid4());second_args[-1]=124
        second=a.resume_issue(*second_args)
        report=a.status_task('harbor',378,'M6-S11-T01','a'*40)
        self.assertEqual(report['request_id'],second['request_id'])
        self.assertEqual(report['ack_id'],124);self.assertEqual(len(self.posts),2)
        self.assertEqual(json.loads(a.issue_lock_file('harbor',378,'M6-S11-T01').read_text())['request_id'],self.parent)

    def test_same_ack_cannot_authorize_next_generation(self):
        first=a.resume_issue(*self.args);self.input=first['input_id']
        self.sessions['sessions'][0].update(id=self.input,lastTurnEndAt=1795104003000)
        with self.assertRaisesRegex(a.Blocked,'ACK_MUST_FOLLOW'):
            a.resume_issue(self.args[0],str(uuid.uuid4()),*self.args[2:])
        self.assertEqual(len(self.posts),1)

    def test_corrupt_or_missing_receipt_reports_unknown_never_replays(self):
        a.resume_issue(*self.args);a.receipt_file('harbor',self.request).unlink()
        r=a.status_task('harbor',378,'M6-S11-T01','a'*40)
        self.assertEqual(r['outcome'],'UNKNOWN')
        self.assertEqual(r['reason'],'INCOMPLETE_RECEIPT_REQUIRES_RECONCILE')
        self.assertEqual(len(self.posts),1)

    def test_unconfirmed_delivery_blocks_even_with_turn_end(self):
        self.record.pop('outbox_delivery')
        a.atomic_private_json(a.receipt_file('harbor',self.parent),self.record)
        original=self.api
        def no_outbox(method,path,payload=None):
            if path.startswith('/v1/inputs?'):return 200,{'inputs':[]}
            return original(method,path,payload)
        with mock.patch.object(a,'call_api',side_effect=no_outbox):
            with self.assertRaisesRegex(a.Blocked,'DELIVERY_NOT_CONFIRMED'):a.resume_issue(*self.args)
        self.assertFalse(self.posts)

    def test_marker_binding_or_source_drift_rejected(self):
        for key,value in [('intent','PICK_NEW'),('issue',999),('pr',999),('head_sha','d'*40),('coordinator','Harbor')]:
            original=copy.deepcopy(self.marker);self.marker[key]=value;self.ack['body']=self.body()
            with self.subTest(key=key),self.assertRaises(a.Blocked):a.resume_issue(*self.args)
            self.marker=original;self.ack['body']=self.body()
        self.assertFalse(self.posts)

    def test_repeated_cursor_is_blocked(self):
        original=self.api
        def repeat(method,path,payload=None):
            if path.startswith('/v1/sessions?'):return 200,{'sessions':[], 'nextCursor':'same'}
            return original(method,path,payload)
        with mock.patch.object(a,'call_api',side_effect=repeat):
            with self.assertRaisesRegex(a.Blocked,'PAGINATION_INVALID'):a.resume_issue(*self.args)
        self.assertFalse(self.posts)

    def test_remote_cli_forwards_pinned_resume_arguments(self):
        import argparse
        args=argparse.Namespace(command='resume-issue',target='harbor',ssh_host='user@mac',
             request_id=self.request,issue=378,task_id='M6-S11-T01',source_sha='a'*40,
             base_sha='b'*40,pr=406,head_sha='c'*40,ack_id=123)
        done=mock.Mock(returncode=0,stdout='{"outcome":"ACCEPTED"}')
        with mock.patch.object(a.subprocess,'run',return_value=done) as run:
            a.run_remote(args)
            command=run.call_args.args[0][-1]
            self.assertIn('resume-issue harbor',command);self.assertIn('--ack-id 123',command)
            self.assertIn('--head-sha '+ 'c'*40,command)
            self.assertEqual(run.call_args.kwargs['encoding'],'utf-8')

    def test_pre_pr_resume_succeeds_without_pr_and_follows_chain(self):
        self.marker.update(pr=None, head_sha=None)
        self.ack['body'] = self.body()
        pre_args = ('harbor', self.request, 378, 'M6-S11-T01', 'a'*40, 'b'*40, None, None, 123)
        def github_no_pr(path):
            if '/pulls/' in path:
                raise AssertionError('pre-PR resume must not fetch PR')
            return self.github(path)
        with mock.patch.object(a, 'github_json', side_effect=github_no_pr):
            r = a.resume_issue(*pre_args)
            self.assertEqual(r['outcome'], 'ACCEPTED')
            self.assertTrue(a.resume_issue(*pre_args)['duplicate'])
        self.assertEqual(len(self.posts), 1)
        self.assertIn('RESUME_CURRENT (pre-PR)', self.posts[0]['text'])
        self.assertIn('123', self.posts[0]['text'])
        report = a.status_task('harbor', 378, 'M6-S11-T01', 'a'*40)
        self.assertEqual(report['request_id'], self.request)
        self.assertIsNone(report['pr'])
        self.assertEqual(report['ack_id'], 123)

    def test_pre_pr_resume_rejects_partial_pr_args_or_mismatched_marker(self):
        with self.assertRaisesRegex(a.Blocked, 'INVALID_RESUME_ARGUMENTS'):
            a.resume_issue('harbor', self.request, 378, 'M6-S11-T01', 'a'*40, 'b'*40, 406, None, 123)
        with self.assertRaisesRegex(a.Blocked, 'INVALID_RESUME_ARGUMENTS'):
            a.resume_issue('harbor', self.request, 378, 'M6-S11-T01', 'a'*40, 'b'*40, None, 'c'*40, 123)
        # Marker still pins pr=406 while caller omits --pr -> must reject
        with self.assertRaisesRegex(a.Blocked, 'RESUME_ACK_PINS_OR_SIGNATURE_INVALID'):
            a.resume_issue('harbor', self.request, 378, 'M6-S11-T01', 'a'*40, 'b'*40, None, None, 123)
        self.assertFalse(self.posts)

    def test_remote_cli_forwards_pre_pr_resume_without_pr_flags(self):
        import argparse
        args = argparse.Namespace(command='resume-issue', target='harbor', ssh_host='user@mac',
             request_id=self.request, issue=378, task_id='M6-S11-T01', source_sha='a'*40,
             base_sha='b'*40, pr=None, head_sha=None, ack_id=123)
        done = mock.Mock(returncode=0, stdout='{"outcome":"ACCEPTED"}')
        with mock.patch.object(a.subprocess, 'run', return_value=done) as run:
            a.run_remote(args)
            command = run.call_args.args[0][-1]
            self.assertIn('resume-issue harbor', command)
            self.assertIn('--ack-id 123', command)
            self.assertNotIn('--pr', command)
            self.assertNotIn('--head-sha', command)
