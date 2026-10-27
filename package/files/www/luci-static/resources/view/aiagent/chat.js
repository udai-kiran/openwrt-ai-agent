'use strict';
'require view';
'require rpc';

var callStatus = rpc.declare({ object: 'aiagent', method: 'status', expect: {} });
var callSend = rpc.declare({ object: 'aiagent', method: 'send', params: ['session', 'message'], expect: {} });
var callPoll = rpc.declare({ object: 'aiagent', method: 'poll', params: ['job'], expect: {} });
var callReset = rpc.declare({ object: 'aiagent', method: 'reset', params: ['session'], expect: {} });

var POLL_MS = 1000;
var MAX_POLLS = 300;

function newSession() {
	return 'w' + Math.random().toString(36).slice(2) + Date.now().toString(36);
}

function sleep(ms) {
	return new Promise(function (resolve) { setTimeout(resolve, ms); });
}

return view.extend({
	load: function () {
		return callStatus().catch(function () { return { ok: false }; });
	},

	render: function (status) {
		var session = newSession();
		var busy = false;

		var log = E('div', {
			'style': 'height:55vh;min-height:240px;overflow-y:auto;padding:8px;border:1px solid rgba(128,128,128,.4);border-radius:4px;'
		});
		var input = E('textarea', {
			'rows': 2,
			'style': 'flex:1;resize:vertical;',
			'placeholder': _('Ask about this router… (Enter to send, Shift+Enter for a new line)')
		});
		var sendBtn = E('button', { 'class': 'cbi-button cbi-button-action' }, _('Send'));
		var resetBtn = E('button', { 'class': 'cbi-button' }, _('New conversation'));

		function addMessage(kind, text) {
			var style = 'margin:6px 0;padding:6px 10px;border-radius:6px;white-space:pre-wrap;word-wrap:break-word;';
			if (kind === 'user')
				style += 'background:rgba(0,120,255,.15);margin-left:15%;';
			else if (kind === 'error')
				style += 'background:rgba(220,50,50,.18);';
			else
				style += 'background:rgba(128,128,128,.15);margin-right:15%;';
			var el = E('div', { 'style': style }, text);
			log.appendChild(el);
			log.scrollTop = log.scrollHeight;
			return el;
		}

		function setBusy(b) {
			busy = b;
			sendBtn.disabled = b;
			input.disabled = b;
		}

		function waitForReply(job) {
			var n = 0;
			function step() {
				if (++n > MAX_POLLS)
					return Promise.reject(new Error(_('Timed out waiting for the model.')));
				return sleep(POLL_MS).then(function () { return callPoll(job); }).then(function (r) {
					if (r.state === 'pending') return step();
					if (r.state === 'done') return r.reply || '';
					return Promise.reject(new Error(r.error || _('Unknown error')));
				});
			}
			return step();
		}

		function send() {
			var text = input.value.trim();
			if (!text || busy) return;
			input.value = '';
			addMessage('user', text);
			var thinking = addMessage('assistant', _('Thinking…'));
			setBusy(true);

			callSend(session, text).then(function (r) {
				if (r.error) throw new Error(r.error);
				return waitForReply(r.job);
			}).then(function (reply) {
				thinking.textContent = reply || _('(empty reply)');
			}).catch(function (err) {
				thinking.remove();
				addMessage('error', err.message || String(err));
			}).then(function () {
				setBusy(false);
				input.focus();
			});
		}

		sendBtn.addEventListener('click', send);
		input.addEventListener('keydown', function (ev) {
			if (ev.key === 'Enter' && !ev.shiftKey) {
				ev.preventDefault();
				send();
			}
		});
		resetBtn.addEventListener('click', function () {
			callReset(session);
			session = newSession();
			log.textContent = '';
			input.focus();
		});

		var banner = [];
		if (!status || !status.ok)
			banner.push(E('div', { 'class': 'alert-message warning' },
				_('The AI Agent service is not running. Start it with: /etc/init.d/aiagent start')));
		else if (!status.configured)
			banner.push(E('div', { 'class': 'alert-message warning' }, [
				_('No model configured yet. '),
				E('a', { 'href': L.url('admin/services/aiagent/settings') }, _('Open Settings'))
			]));

		return E([
			E('h2', _('AI Agent')),
			E('div', { 'class': 'cbi-map-descr' },
				_('Ask questions about this router. The assistant can only read its state; it cannot change anything.')),
			E('div', banner),
			log,
			E('div', { 'style': 'display:flex;gap:8px;margin-top:8px;align-items:flex-end;' }, [input, sendBtn]),
			E('div', { 'style': 'margin-top:8px;' }, resetBtn)
		]);
	},

	handleSave: null,
	handleSaveApply: null,
	handleReset: null
});
