#!/usr/bin/env python3
"""Deterministic native-protocol fixture; never runs a model or a command."""
import json
import os
import sys
from pathlib import Path

claude = '--print' in sys.argv
question = False
log = Path('provider-requests.jsonl')
def emit(value):
    print(json.dumps(value), flush=True)
def note(value):
    with log.open('a') as f:
        f.write(json.dumps(value) + '\n')
def end():
    if claude:
        emit({'type':'stream_event','event':{'type':'message_start','message':{'id':'answer'}}})
        emit({'type':'stream_event','event':{'type':'content_block_start','index':0,'content_block':{'type':'text','text':''}}})
        emit({'type':'stream_event','event':{'type':'content_block_delta','index':0,'delta':{'type':'text_delta','text':'Partial'}}})
        emit({'type':'assistant','message':{'id':'answer','content':[{'type':'text','text':'Verified **Rust** response'}]}})
        emit({'type':'result','uuid':'turn-1','is_error':False})
    else:
        emit({'method':'item/started','params':{'threadId':'thread-1','turnId':'turn-1','item':{'type':'agentMessage','id':'answer','text':''}}})
        emit({'method':'item/agentMessage/delta','params':{'threadId':'thread-1','turnId':'turn-1','itemId':'answer','delta':'Partial'}})
        emit({'method':'item/completed','params':{'threadId':'thread-1','turnId':'turn-1','item':{'type':'agentMessage','id':'answer','text':'Verified **Rust** response'}}})
        emit({'method':'turn/completed','params':{'threadId':'thread-1','turn':{'id':'turn-1','status':'completed'}}})
# Report the session environment this child was given, so a test can check
# that it crossed from the create request into the spawned process.
note({'env': {k: v for k, v in os.environ.items() if k.startswith('MUSTER_TEST_')}})
for line in sys.stdin:
    f = json.loads(line)
    note(f)
    if claude:
        if f.get('type') == 'control_request':
            emit({'type':'control_response','response':{'request_id':f['request_id'],'subtype':'success','response':{}}})
            if f['request']['subtype'] == 'interrupt':
                emit({'type':'result','uuid':'turn-1','is_error':True})
        elif f.get('type') == 'user':
            question = 'question' in str(f)
            emit({'type':'system','subtype':'init','session_id':'claude-session-1'})
            if 'lost receipt' in str(f):
                sys.exit(0)
            emit(f)
            if question:
                emit({'type':'control_request','request_id':'approval-1','request':{'subtype':'can_use_tool','tool_name':'AskUserQuestion','input':{'questions':[{'question':'Choose mode','header':'Mode','multiSelect':False,'options':[{'label':'Safe','description':'default'}]}]}}})
            else:
                emit({'type':'control_request','request_id':'approval-1','request':{'subtype':'can_use_tool','tool_name':'Bash','input':{'command':'echo requested'}}})
        elif f.get('type') == 'control_response':
            assert f['response']['response']['behavior'] in ('allow','deny')
            end()
    else:
        method = f.get('method')
        if method == 'initialize':
            emit({'id':f['id'],'result':{'userAgent':'fixture'}})
        elif method in ('thread/start','thread/resume'):
            assert f['params']['approvalPolicy'] == 'on-request'
            assert f['params']['sandbox'] == 'workspace-write'
            emit({'id':f['id'],'result':{'thread':{'id':'thread-1'}}})
        elif method == 'turn/start':
            question = 'question' in str(f)
            if 'lost receipt' in str(f):
                sys.exit(0)
            emit({'method':'turn/started','params':{'threadId':'thread-1','turn':{'id':'turn-1','status':'inProgress'}}})
            emit({'id':f['id'],'result':{'turn':{'id':'turn-1'}}})
            if question:
                emit({'id':99,'method':'item/tool/requestUserInput','params':{'threadId':'thread-1','turnId':'turn-1','questions':[{'id':'0','question':'Choose mode','header':'Mode','isOther':True,'options':[{'label':'Safe','description':'default'}]}]}})
            else:
                emit({'id':99,'method':'item/commandExecution/requestApproval','params':{'threadId':'thread-1','turnId':'turn-1','itemId':'command-1','command':'echo requested','availableDecisions':['accept','decline']}})
        elif method == 'turn/interrupt':
            emit({'id':f['id'],'result':{}})
            emit({'method':'turn/completed','params':{'threadId':'thread-1','turn':{'id':'turn-1','status':'interrupted'}}})
        elif f.get('id') == 99 and 'result' in f:
            if question:
                assert f['result']['answers'] == {'0': {'answers': ['Safe']}}
            else:
                assert f['result']['decision'] in ('accept','decline')
            emit({'method':'serverRequest/resolved','params':{'threadId':'thread-1','requestId':99}})
            end()
