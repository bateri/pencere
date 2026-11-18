--@ size 520 240
--@ crop demo
local page = require("../_page")

return page(function()
	return pencere.div():w(340):gap(8)
		:child(pencere.text("Deploy finished"):text_size(18):weight(700))
		:child(pencere.text("3 services, 12 seconds"):color("dim"))
		:child(pencere.text("Every line of text is one line unless it wraps, and then its box is as tall as the lines it needs."):wrap())
		:child(pencere.text("/usr/local/bin/postgres --serve --port 3032 --config /etc/postgres.conf"):truncate():color("dim"))
		:child(pencere.div():row():gap(12)
			:child(pencere.text("italic"):italic())
			:child(pencere.text("underline"):underline())
			:child(pencere.text("strike"):strike())
			:child(pencere.text("accent"):color("accent")))
end)
