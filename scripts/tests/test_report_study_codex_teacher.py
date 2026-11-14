"""No network/model calls: verify the exact requested backend and fresh output."""
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest import mock

sys.path.insert(0,str(Path(__file__).parents[1]))
import report_study
import report_study_codex_teacher as teacher


class CodexTeacherTests(unittest.TestCase):
    def setUp(self):
        self.temporary=tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root=Path(self.temporary.name)
        self.input,self.output=self.root/'input.json',self.root/'candidate.json'
        self.text='A controlled object remains readable after membership removal.'
        report_study.write_json(self.input,{'schema_example':{},'untrusted_report':{
            'report_id':101,'text':self.text,'basis':'narrative',
            'content_sha256':report_study.digest(self.text.encode())}})

    def test_exact_model_and_light_effort_disable_tools_and_delegation(self):
        args=teacher.command('codex',self.root,self.root/'schema.json',self.output)
        self.assertEqual(args[args.index('--model')+1],'gpt-6.1-sol')
        self.assertIn('model_reasoning_effort="low"',args)
        self.assertIn('--ignore-user-config',args)
        self.assertIn('--ephemeral',args)
        for name in ['shell_tool','unified_exec','multi_agent','apps','browser_use','computer_use']:
            self.assertTrue(any(args[i:i+2]==['--disable',name] for i in range(len(args)-1)))

    def test_success_requires_a_completed_turn_and_fresh_output_and_records_usage(self):
        def complete(args,**kwargs):
            self.assertIn(self.text,kwargs['input'].decode())
            report_study.write_json(self.output,{'version':1,'lessons':[]})
            return subprocess.CompletedProcess(args,0,b'{"type":"turn.completed","usage":{"input_tokens":12,"output_tokens":3}}\n',b'')
        with mock.patch.object(teacher.subprocess,'run',side_effect=complete):
            usage=teacher.teach(self.input,self.output,'codex')
        self.assertEqual(usage['usage']['input_tokens'],12)
        self.assertTrue(self.output.with_suffix('.usage.json').exists())
        with mock.patch.object(teacher.subprocess,'run') as backend:
            with self.assertRaises(ValueError):teacher.teach(self.input,self.output,'codex')
            backend.assert_not_called()

    def test_backend_failure_never_substitutes_another_model(self):
        with mock.patch.object(teacher.subprocess,'run',return_value=subprocess.CompletedProcess([],1,b'',b'')) as backend:
            with self.assertRaisesRegex(ValueError,'no fallback'):
                teacher.teach(self.input,self.output,'codex')
        self.assertEqual(backend.call_count,1)

    def test_source_hash_tampering_fails_before_a_model_request(self):
        value=json.loads(self.input.read_bytes())
        value['untrusted_report']['text']+='tampered'
        report_study.write_json(self.input,value)
        with mock.patch.object(teacher.subprocess,'run') as backend:
            with self.assertRaises(ValueError):teacher.teach(self.input,self.output,'codex')
            backend.assert_not_called()


if __name__=='__main__':unittest.main()
