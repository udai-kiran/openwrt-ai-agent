'use strict';
'require view';
'require form';

return view.extend({
	render: function () {
		var m, s, o;

		m = new form.Map('aiagent', _('AI Agent'),
			_('Read-only chat assistant for this router. Any OpenAI-compatible endpoint works.'));

		s = m.section(form.NamedSection, 'main', 'aiagent');
		s.addremove = false;

		o = s.option(form.Value, 'endpoint', _('API endpoint'),
			_('Base URL, for example https://api.openai.com/v1 or http://192.168.1.10:11434/v1'));
		o.rmempty = false;
		o.validate = function (section_id, value) {
			return /^https?:\/\/\S+$/.test(value) ? true : _('Must start with http:// or https://');
		};

		o = s.option(form.Value, 'model', _('Model'));
		o.rmempty = false;
		o.placeholder = 'gpt-4o-mini';

		o = s.option(form.Value, 'api_key', _('API key'),
			_('Stored in /etc/config/aiagent (root-only). Leave empty for endpoints without auth.'));
		o.password = true;

		o = s.option(form.Value, 'max_tool_rounds', _('Max tool rounds'),
			_('How many times the model may query the router per question.'));
		o.datatype = 'range(1,20)';
		o.placeholder = '6';

		o = s.option(form.Value, 'timeout', _('LLM timeout (seconds)'));
		o.datatype = 'range(5,600)';
		o.placeholder = '60';

		o = s.option(form.TextValue, 'system_prompt', _('System prompt'),
			_('Optional. Leave empty to use the built-in read-only prompt.'));
		o.rows = 6;
		o.optional = true;

		return m.render();
	}
});
