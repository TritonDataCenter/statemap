/*
 * Copyright 2017, Joyent, Inc.
 */

/*
 * This file is dropped into the generated SVG -- and if you're looking at
 * the generated SVG and wondering where this comes from, look for
 * statemap-svg.js in statemap's lib directory.
 */

var g_transMatrix = [1, 0, 0, 1, 0, 0];		/* transform of statemap */
var g_svgDoc;					/* our SVG document */
var g_offset;					/* x offset of statemap */
var g_timelabel;				/* label for time spanned */
var g_timebar;					/* timebar, if any */
var g_statebar;					/* statebar, if any */
var g_statemap;					/* statemap element */
var g_height;					/* pixel height of statemap */
var g_width;					/* pixel width of statemap */
var g_entities;					/* array of entities */

var g_data;

var timeunits = function (timeval)
{
	var i, rem;
	var suffixes = [ 'ns', 'μs', 'ms', 's' ];

	if (timeval === 0)
		return ('0');

	for (i = 0; timeval > 1000 && i < suffixes.length - 1; i++)
		timeval /= 1000;

	rem = Math.floor((timeval - Math.floor(timeval)) * 1000);

	return (Math.floor(timeval) + '.' +
	    (rem < 100 ? '0' : '') + (rem < 10 ? '0' : '') + rem +
	    suffixes[i]);
};

var timeFromMapX = function (mapX)
{
	var base, offs;
	var timeWidth = globals.timeWidth;

	/*
	 * Our base (in nanoseconds) is our X offset in the transformation
	 * matrix as a ratio of our total (scaled) width, times our timeWidth.
	 */
	base = (-g_transMatrix[4] / (g_transMatrix[0] * g_width)) * timeWidth;

	/*
	 * Our offset (in nanoseconds) is the X offset within the statemap
	 * as a ratio of the statemap width, times the number of nanoseconds
	 * visible in the statemap (which itself is the timeWidth divided by
	 * our scaling factor).
	 */
	offs = (mapX / g_width) * (timeWidth / g_transMatrix[0]);

	return (base + offs);
};

var timeToText = function (time)
{
	var t;

	if (g_transMatrix[0] === 1 && globals.begin === 0) {
		t = 'offset = ' + timeunits(time);
	} else {
		t = 'offset = ' + timeunits(time) + ', ' +
		    timeunits(time + globals.begin) + ' overall';
	}

	if (globals.start) {
		var s = globals.start[0] +
		    (time + globals.start[1]) / 1000000000;

		t += ' (Epoch + ' + Math.floor(s) + 's)';
	}

	return (t);
};

var timeSetSpanLabel = function ()
{
	var t = 'span = ' + timeunits(globals.timeWidth / g_transMatrix[0]);

	if (g_transMatrix[0] != 1 || globals.begin !== 0)
		t += '; ' + timeToText(timeFromMapX(0));

	g_timelabel.textContent = t;
};

var init = function (evt)
{
	var i, position = 0;

	g_svgDoc = evt.target.ownerDocument;
	g_statemap = g_svgDoc.getElementById('statemap');
	g_height = globals.pixelHeight;
	g_width = globals.pixelWidth;

	g_offset = evt.target.getAttributeNS(null, 'width') - g_width;

	g_timelabel = g_svgDoc.getElementById('statemap-timelabel');
	timeSetSpanLabel();

	g_timebar = undefined;
	g_entities = [];

	/*
	 * Iterate over our statemap's children, looking for entities.
	 */
	for (i = 0; i < g_statemap.childNodes.length; i++) {
		var id = g_statemap.childNodes[i].id;

		if (!id || id.indexOf(globals.entityPrefix) !== 0)
			continue;

		g_entities[id] = {
			name: id.substr(globals.entityPrefix.length),
			element: g_statemap.childNodes[i],
			position: position++
		};
	}
};

var entityDatum = function (entity, idx)
{
	var datum = g_data[entity.name][idx];
	var rval = { time: datum.t };

	if (datum.s instanceof Object) {
		rval.states = datum.s;
	} else {
		rval.state = datum.s;
	}

	if (idx + 1 < g_data[entity.name].length) {
		rval.etime = g_data[entity.name][idx + 1].t;
	} else {
		rval.etime = globals.timeWidth + globals.begin;
	}

	return (rval);
};

var entityBreakdown = function (entity, time, etime)
{
	var data = g_data[entity.name];
	var rval = {};

	var idx, length = data.length;
	var floor = 0;
	var ceil = length;
	var datum, t, span, state;

	if (length === 0 || data[0].t > time)
		return ({});

	/*
	 * Binary search our data until we find a datum that contains the
	 * specified time.
	 */
	for (;;) {
		idx = floor + Math.floor((ceil - floor) / 2);

		if (data[idx].t > time) {
			ceil = idx;
			continue;
		}

		if (idx + 1 == length || data[idx + 1].t > time)
			break;

		floor = idx;
	}

	/*
	 * If we don't have a specified etime, we want to just return the state
	 * breakdown at the specified time.
	 */
	if (!etime) {
		datum = data[idx];

		if (datum.s instanceof Object)
			return (datum.s);

		rval[datum.s] = 1.0;

		return (rval);
	}

	/*
	 * Now we're going to iterate forward until we get past our specified
	 * etime.
	 */
	for (; idx < length; idx++) {
		datum = data[idx];

		if (datum.t > etime)
			break;

		if ((t = datum.t) < time)
			t = time;

		if (idx + 1 == length || data[idx + 1].t > etime) {
			/*
			 * This datum contains the end of the range; our span
			 * is our etime minus this datum's start time (or our
			 * specified time, whichever is greater).
			 */
			span = etime - t;
		} else {
			/*
			 * The end of the datum is covered by the range; our
			 * span is the time width of the datum.
			 */
			span = data[idx + 1].t - t;
		}

		/*
		 * Express our span as a ratio of the overall time.
		 */
		span /= (etime - time);

		if (datum.s instanceof Object) {
			for (state in datum.s) {
				if (!rval.hasOwnProperty(state))
					rval[state] = 0;

				rval[state] += (datum[state] * span);
			}
		} else {
			state = datum.s;

			if (!rval.hasOwnProperty(state))
				rval[state] = 0;

			rval[state] += span;
		}
	}

	return (rval);
};

var statebarCreateBar = function (statebar, x1, y1, x2, y2)
{
	var parent = statebar.parent;

	var bar = g_svgDoc.createElementNS(parent.namespaceURI, 'line');
	bar.classList.add('statemap-statebar');
	bar.x1.baseVal.value = x1;
	bar.y1.baseVal.value = y1;
	bar.x2.baseVal.value = x2;
	bar.y2.baseVal.value = y2;
	parent.appendChild(bar);
	statebar.bars.push(bar);
};

var statebarCreate = function (elem, idx)
{
	var parent = g_statemap.parentNode.parentNode;
	var statebar = { parent: parent, hidden: false };
	var entity = g_entities[elem.parentNode.id];
	var datum = entityDatum(entity, idx);
	var pos = entity.position;
	var x = globals.lmargin - 2;
	var y = globals.tmargin + (pos * globals.stripHeight);
	var elbow = { x: 8, y: 10 };
	var nudge = { x: 3, y: 2 };
	var direction = 1, anchor;
	var text;

	if (pos * globals.stripHeight < globals.pixelHeight / 2) {
		direction = 1;
		anchor = 'end';
	} else {
		direction = -1;
		anchor = 'start';
	}

	statebar.bars = [];

	/*
	 * We have three bars to draw:  our bar that runs the height of the
	 * strip, followed by our elbow.
	 */
	statebarCreateBar(statebar, x, y, x, y + globals.stripHeight);

	y += 0.5 * globals.stripHeight;
	statebarCreateBar(statebar, x - elbow.x, y, x, y);

	x -= elbow.x;
	statebarCreateBar(statebar, x, y, x, y + (elbow.y * direction));

	/*
	 * Now create the text at the end of the elbow.
	 */
	y += (elbow.y + nudge.y) * direction;
	x += nudge.x;
	text = g_svgDoc.createElementNS(parent.namespaceURI, 'text');
	text.classList.add('sansserif');
	text.classList.add('statemap-statetext');

	var t = globals.entityKind + ' ' + entity.name;

	if (globals.entities[entity.name].description)
		t += ' (' + globals.entities[entity.name].description + ')';

	if (datum.hasOwnProperty('state')) {
		t += ', ' + globals.states[datum.state].name;
	} else {
		var i, total = 0, max = 0, maxstate;

		for (i in datum.states) {
			total += datum.states[i];

			if (datum.states[i] > max) {
				maxstate = i;
				max = datum.states[i];
			}
		}

		t += ', ' + Math.floor((datum.states[maxstate] / total) * 100);
		t += '% ' + globals.states[maxstate].name;
	}

	t += ' at ' + timeunits(datum.time);
	t += ' for ' + timeunits(datum.etime - datum.time);

	text.appendChild(g_svgDoc.createTextNode(t));
	text.setAttributeNS(null, 'x', x);
	text.setAttributeNS(null, 'y', y);
	text.setAttributeNS(null, 'transform',
	    'rotate(270,' + x + ',' + y + ')');
	text.setAttributeNS(null, 'text-anchor', anchor);
	text.addEventListener('click',
	    function () { statebarRemove(statebar); });

	parent.appendChild(text);

	statebar.text = text;

	return (statebar);
};

var statebarRemove = function (statebar)
{
	var i;

	if (!statebar)
		return;

	if (statebar.bars) {
		for (i = 0; i < statebar.bars.length; i++)
			statebar.parent.removeChild(statebar.bars[i]);
		statebar.parent.removeChild(statebar.text);
	}

	statebar.bars = undefined;
	statebar.text = undefined;
};

var timebarRemove = function (timebar)
{
	if (!timebar)
		return;

	if (timebar.bar && !timebar.hidden) {
		timebar.parent.removeChild(timebar.bar);
		timebar.parent.removeChild(timebar.text);
	}

	if (timebar.breakdown) {
		var i;

		for (i = 0; i < timebar.breakdown.length; i++) {
			var elem = timebar.breakdown[i];
			elem.parentNode.removeChild(elem);
		}
	}

	timebar.bar = undefined;
	timebar.text = undefined;
	timebar.breakdown = undefined;
};

var timebarSetBarLocation = function (bar, mapX)
{
	var absX = mapX + g_offset;
	var nubheight = 15;

	bar.x1.baseVal.value = absX;
	bar.y1.baseVal.value = globals.tmargin - nubheight;
	bar.x2.baseVal.value = absX;
	bar.y2.baseVal.value = globals.tmargin + g_height;
};

var timebarSetTextLocation = function (text, mapX)
{
	var absX = mapX + g_offset;
	var nudge = { x: 3, y: 5 };
	var direction, anchor;
	var time;

	/*
	 * The side of the timebar that we actually render the text containing
	 * the offset and the time depends on the location of our timebar with
	 * respect to the center of the visible statemap.
	 */
	if (mapX < (g_width / 2)) {
		direction = 1;
		anchor = 'start';
	} else {
		direction = -1;
		anchor = 'end';
	}

	text.setAttributeNS(null, 'x', absX + (direction * nudge.x));
	text.setAttributeNS(null, 'y', globals.tmargin - nudge.y);
	text.setAttributeNS(null, 'text-anchor', anchor);

	time = timeFromMapX(mapX);
	text.childNodes[0].textContent = timeToText(time);

	return (time);
};

var timebarHide = function (timebar)
{
	if (!timebar || timebar.hidden || !timebar.bar)
		return;

	timebar.parent.removeChild(timebar.bar);
	timebar.parent.removeChild(timebar.text);
	timebar.hidden = true;
};

var timebarShow = function (timebar)
{
	var mapX;

	if (!timebar || !timebar.hidden)
		return;

	/*
	 * We take the ratio of the time of the timebar of the total time
	 * width times the width times the scale, and then add that to the
	 * X offset in the transformation matrix.
	 */
	mapX = ((timebar.time / globals.timeWidth) * g_width *
	    g_transMatrix[0]) + g_transMatrix[4];

	if (mapX < 0 || mapX >= g_width)
		return;

	timebarSetBarLocation(timebar.bar, mapX);
	timebarSetTextLocation(timebar.text, mapX);

	timebar.parent.appendChild(timebar.bar);
	timebar.parent.appendChild(timebar.text);
	timebar.hidden = false;
};

var timebarSetMiddle = function (timebar)
{
	var mapX = g_width / 2;

	if (!timebar || !timebar.bar)
		return;

	/*
	 * This is just an algebraic rearrangement of the mapX calculation
	 * in timebarShow(), above.
	 */
	g_transMatrix[4] = -(((timebar.time / globals.timeWidth) * g_width *
	    g_transMatrix[0]) - mapX);
};

var timebarSetBreakdown = function (time)
{
	var breakdown, state, total = {};
	var entity;
	var sum = 0;
	var rval = [];

	time += globals.begin;

	for (entity in g_entities) {
		breakdown = entityBreakdown(g_entities[entity], time);

		for (state in breakdown) {
			if (!total.hasOwnProperty(state))
				total[state] = 0;

			sum += breakdown[state];
			total[state] += breakdown[state];
		}
	}

	for (state in total) {
		var legend, parent, text;
		var x, y, width, height, t;
		var nudge = 3;

		legend = g_svgDoc.getElementById('statemap-legend-' + state);
		parent = legend.parentNode;

		x = parseInt(legend.getAttributeNS(null, 'x'), 10);
		y = parseInt(legend.getAttributeNS(null, 'y'), 10);
		width = parseInt(legend.getAttributeNS(null, 'width'), 10);
		height = parseInt(legend.getAttributeNS(null, 'height'), 10);

		t = Math.floor(total[state]) + ' (' +
		    Math.floor((total[state] / sum) * 100) + '%)';

		text = g_svgDoc.createElementNS(parent.namespaceURI, 'text');
		text.classList.add('sansserif');
		text.classList.add('statemap-timebreaktext');

		text.appendChild(g_svgDoc.createTextNode(t));
		text.setAttributeNS(null, 'x', x + (width / 2));
		text.setAttributeNS(null, 'y', y + (height / 2) + nudge);
		text.setAttributeNS(null, 'text-anchor', 'middle');

		parent.appendChild(text);
		rval.push(text);
	}

	return (rval);
};

var timebarCreate = function (mapX)
{
	var parent = g_statemap.parentNode.parentNode;
	var bar, text;
	var timebar = { parent: parent, hidden: false };

	bar = g_svgDoc.createElementNS(parent.namespaceURI, 'line');
	bar.classList.add('statemap-timebar');

	timebarSetBarLocation(bar, mapX);
	parent.appendChild(bar);

	text = g_svgDoc.createElementNS(parent.namespaceURI, 'text');
	text.classList.add('sansserif');
	text.classList.add('statemap-timetext');
	text.appendChild(g_svgDoc.createTextNode(''));

	timebar.time = timebarSetTextLocation(text, mapX);
	timebar.breakdown = timebarSetBreakdown(timebar.time);

	text.addEventListener('click', function () { timebarRemove(timebar); });
	parent.appendChild(text);

	timebar.bar = bar;
	timebar.text = text;

	return (timebar);
};

var mapclick = function (evt, idx)
{
	var x = evt.clientX - g_offset;

	timebarRemove(g_timebar);
	g_timebar = timebarCreate(x);

	statebarRemove(g_statebar);
	g_statebar = statebarCreate(evt.target, idx);
};

var pan = function (dx, dy)
{
	var minX = -(g_width * g_transMatrix[0] - g_width);
	var minY = -(g_height * g_transMatrix[0] - g_height);

	g_transMatrix[4] += dx;
	g_transMatrix[5] += dy;

	timebarHide(g_timebar);

	if (g_transMatrix[4] > 0)
		g_transMatrix[4] = 0;

	if (g_transMatrix[4] < minX)
		g_transMatrix[4] = minX;

	if (g_transMatrix[5] > 0)
		g_transMatrix[5] = 0;

	if (g_transMatrix[5] < minY)
		g_transMatrix[5] = minY;

	timeSetSpanLabel();

	var newMatrix = 'matrix(' +  g_transMatrix.join(' ') + ')';
	g_statemap.setAttributeNS(null, 'transform', newMatrix);
	timebarShow(g_timebar);
};

var zoom = function (scale)
{
	var i;

	timebarHide(g_timebar);

	for (i = 0; i < g_transMatrix.length; i++) {
		/*
		 * We don't scale the Y direction on a zoom.
		 */
		if (i != 3)
			g_transMatrix[i] *= scale;
	}

	var minX = -(g_width * g_transMatrix[0] - g_width);
	var minY = -(g_height * g_transMatrix[0] - g_height);

	g_transMatrix[4] += (1 - scale) * g_width / 2;
	timebarSetMiddle(g_timebar);

	if (g_transMatrix[4] > 0)
		g_transMatrix[4] = 0;

	if (g_transMatrix[4] < minX)
		g_transMatrix[4] = minX;

	if (g_transMatrix[5] > 0)
		g_transMatrix[5] = 0;

	if (g_transMatrix[5] < minY)
		g_transMatrix[5] = minY;

	if (g_transMatrix[0] < 1)
		g_transMatrix = [1, 0, 0, 1, 0, 0];

	timeSetSpanLabel();

	var newMatrix = 'matrix(' +  g_transMatrix.join(' ') + ')';
	g_statemap.setAttributeNS(null, 'transform', newMatrix);
	timebarShow(g_timebar);
};
