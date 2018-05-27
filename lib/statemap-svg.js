/*
 * Copyright 2018, Joyent, Inc.
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
var g_statesel;					/* state selection, if any */
var g_tagsel;					/* tag selection, if any */
var g_tagvalsel;				/* tag val selection, if any */

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

	g_offset = evt.target.getAttributeNS(null, 'width') -
	    (g_width + globals.tagWidth);

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

	/*
	 * Dynamically change the styling of the highlight rectangle.
	 */
	var highlight = g_svgDoc.getElementById('statemap-highlight');
	highlight.classList.add('statemap-highlight');
};

var entityForEachDatum = function (entity, time, etime, func)
{
	var data = g_data[entity.name];

	var idx, length = data.length;
	var floor = 0;
	var ceil = length;
	var datum, t, span;

	if (length === 0 || (data[0].t > time && !etime))
		return;

	if (data[0].t > time) {
		idx = 0;
	} else {
		/*
		 * Binary search our data until we find a datum that contains
		 * the start of our time range.
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
	}

	/*
	 * If we don't have a specified etime, we have found the datum that
	 * contains the time; just call our function and return.
	 */
	if (!etime) {
		func(data[idx], idx, 1);
		return;
	}

	/*
	 * Now we're going to iterate forward, calling our function until we
	 * get past our specified etime.
	 */
	for (; idx < length; idx++) {
		datum = data[idx];

		if (datum.t > etime)
			return;

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

		func(datum, idx, span);
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

	if (pos * globals.stripHeight <
	    (globals.totalHeight - globals.tmargin) / 2) {
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
	text.addEventListener('click', function () {
		statebarRemove(statebar);
		stateselUpdate();
	});

	parent.appendChild(text);

	statebar.text = text;
	statebar.entity = entity;

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
	statebar.entity = undefined;
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

	var click = function (s) {
		return (function (evt) { legendclick(evt, s); });
	};

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
		text.addEventListener('click',
		    click(globals.states[state].value));

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

	text.addEventListener('click', function () {
		timebarRemove(timebar);
		stateselUpdate();
	});

	parent.appendChild(text);

	timebar.bar = bar;
	timebar.text = text;

	return (timebar);
};

var stateselTagvalSelect = function (evt, tagval)
{
	var tagdefs = {};
	var i, entity;
	var state;
	var child;
	var highlight = 'statemap-tagbox-select-highlighted';

	if (g_statesel == undefined)
		return;

	state = g_statesel.state;

	if (g_tagvalsel && g_tagvalsel.selected) {
		for (i = 0; i < g_tagvalsel.selected.length; i++) {
			child = g_tagvalsel.selected[i];
			child.removeAttribute('fill-opacity');
		}

		if (g_tagvalsel.element)
			g_tagvalsel.element.classList.remove(highlight);

		/*
		 * If our selection matches the selection that we have already
		 * made, then we are unselecting this tag value; we need only
		 * return.
		 */
		if (g_tagvalsel.tag == g_tagsel.tag &&
		    g_tagvalsel.tagval == tagval) {
			g_tagvalsel = undefined;
			return;
		}
	}

	g_tagvalsel = { selected: [], tag: g_tagsel.tag, tagval: tagval };

	evt.target.classList.add(highlight);
	g_tagvalsel.element = evt.target;

	/*
	 * Iterate over all of our tag definitions, looking for a match where
	 * the specified tag (for the specified state) matches the specified
	 * tag value.
	 */
	for (i = 0; i < globals.tags.length; i++) {
		if (globals.tags[i].state != state)
			continue;

		if (globals.tags[i][g_tagsel.tag] != tagval)
			continue;

		tagdefs[i] = true;
	}

	/*
	 * Now for each entity, we will plow through every rectangle.
	 */
	for (entity in g_entities) {
		var elem = g_entities[entity].element;
		var data = g_data[g_entities[entity].name];
		var j = 0;

		for (i = 0; i < elem.childNodes.length; i++) {
			child = elem.childNodes[i];

			if (child.nodeName != 'rect')
				continue;

			var datum = data[j++];
			var tag;

			if (datum.s instanceof Object) {
				if (!datum.s[state])
					continue;
			} else {
				if (datum.s != state)
					continue;
			}

			if (!datum.g)
				continue;

			var ratio = 0;

			for (tag in datum.g) {
				if (tagdefs[tag])
					ratio += datum.g[tag];
			}

			if (ratio === 0)
				continue;

			/*
			 * At this point we have found a rectangle that we
			 * want to color, and we know the degree that we
			 * want to color it!
			 */
			child.setAttributeNS(null, 'fill-opacity', 1 - ratio);
			g_tagvalsel.selected.push(child);
		}
	}
};

var stateselUpdate = function ()
{
	var base, etime, nentities = 0;
	var state, entity;
	var bytag = {}, tagval;
	var header, i;

	if (g_statesel == undefined)
		return;

	state = g_statesel.state;

	var sum = function (datum, id, span) {
		var tid, tag;

		if (!(datum.s instanceof Object)) {
			if (datum.s != state)
				return;
		} else {
			var ratio;

			if (!(ratio = datum.s[state]))
				return;
		}

		if (!datum.g)
			return;

		for (tid in datum.g) {
			tag = globals.tags[tid];

			if (tag.state != state)
				continue;

			if (!(tagval = tag[g_tagsel.tag]))
				continue;

			if (!bytag[tagval])
				bytag[tagval] = 0;

			bytag[tagval] += span * datum.g[tid];
		}
	};

	if (!g_tagsel)
		return;

	header = '';

	if (g_statebar && g_statebar.entity) {
		header = globals.entityKind + ' ' +
		    g_statebar.entity.name + ' ';
	}

	header += 'by ' + g_tagsel.tag + ' ';

	if (g_timebar && g_timebar.bar) {
		base = g_timebar.time + globals.begin;
		etime = 0;
		header += 'at ' + timeunits(g_timebar.time);
	} else {
		base = timeFromMapX(0) + globals.begin;
		etime = timeFromMapX(g_width) + globals.begin;
		header += 'over span';
	}

	header = header.charAt(0).toUpperCase() + header.substr(1) + ':';

	if (g_statebar && g_statebar.entity) {
		entityForEachDatum(g_statebar.entity, base, etime, sum);
		nentities++;
	} else {
		/*
		 * For each entity, we need to determine the amount of time
		 * in our selected state.
		 */
		for (entity in g_entities) {
			entityForEachDatum(g_entities[entity],
			    base, etime, sum);
			nentities++;
		}
	}

	var sorted = Object.keys(bytag).sort(function (lhs, rhs) {
		if (bytag[lhs] < bytag[rhs]) {
			return (1);
		} else if (bytag[lhs] > bytag[rhs]) {
			return (-1);
		} else {
			return (0);
		}
	});

	var divisor;

	if (etime === 0) {
		divisor = nentities;
	} else {
		divisor = (etime - base) * nentities;
	}

	var x = g_statesel.x;
	var y = g_statesel.y + 10;

	var elem = g_svgDoc.getElementById('statemap-tagbox-select');

	while (elem.childNodes.length > 0)
		elem.removeChild(elem.childNodes[0]);

	if (g_tagvalsel && g_tagvalsel.element)
		g_tagvalsel.element = undefined;

	var text = g_svgDoc.createElementNS(elem.namespaceURI, 'text');
	text.classList.add('statemap-tagbox-select-header');
	text.classList.add('sansserif');

	text.appendChild(g_svgDoc.createTextNode(header));
	text.setAttributeNS(null, 'x', x);
	text.setAttributeNS(null, 'y', y);
	elem.appendChild(text);
	y += 9;

	var line = g_svgDoc.createElementNS(elem.namespaceURI, 'line');
	line.classList.add('statemap-tagbox-select-header-line');
	line.x1.baseVal.value = x - 2;
	line.y1.baseVal.value = y;
	line.x2.baseVal.value = g_statesel.x2;
	line.y2.baseVal.value = y;
	elem.appendChild(line);
	y += 18;

	var bmargin = 60;
	var ttl = 0;
	var ellipsis = false;

	var click = function (tv) {
		return (function (evt) { stateselTagvalSelect(evt, tv); });
	};

	for (i = 0; i <= sorted.length; i++) {
		var t, perc;

		if (i < sorted.length) {
			perc = (bytag[sorted[i]] / divisor) * 100.0;
			tagval = sorted[i];
			ttl += perc;

			if (y > globals.totalHeight - bmargin) {
				if (ellipsis)
					continue;

				ellipsis = true;
				tagval = '...';
			}
		} else {
			perc = ttl;
			tagval = 'total';

			y -= 5;
			line = g_svgDoc.createElementNS(elem.namespaceURI,
			    'line');
			line.classList.add('statemap-tagbox-select-sum-line');
			line.x1.baseVal.value = x - 2;
			line.y1.baseVal.value = y;
			line.x2.baseVal.value = g_statesel.x2;
			line.y2.baseVal.value = y;
			elem.appendChild(line);
			y += 15;
		}

		if (i != sorted.length && ellipsis) {
			t = '...';
		} else {
			t = Math.trunc(perc) + '.' +
			    (Math.round(perc * 100) % 100) + '%';
		}

		text = g_svgDoc.createElementNS(elem.namespaceURI, 'text');
		text.classList.add('statemap-tagbox-select-perc');
		text.classList.add('sansserif');
		text.appendChild(g_svgDoc.createTextNode(t));
		text.setAttributeNS(null, 'x', x + 45);
		text.setAttributeNS(null, 'y', y);
		elem.appendChild(text);

		text = g_svgDoc.createElementNS(elem.namespaceURI, 'text');
		text.classList.add('statemap-tagbox-select');
		text.classList.add('sansserif');
		text.appendChild(g_svgDoc.createTextNode(tagval));
		text.setAttributeNS(null, 'x', x + 50);
		text.setAttributeNS(null, 'y', y);

		/*
		 * If we already have a tag value selection and it matches
		 * what we're about to display, indicate as much by
		 * highlighting it.
		 */
		if (g_tagvalsel && g_tagvalsel.tag == g_tagsel.tag &&
		    g_tagvalsel.tagval == tagval) {
			var highlight = 'statemap-tagbox-select-highlighted';
			text.classList.add(highlight);
			g_tagvalsel.element = text;
		}

		text.addEventListener('click', click(tagval));

		elem.title = t;
		elem.appendChild(text);
		y += 15;
	}
};

var stateselTagSelect = function (evt, tag)
{
	var elem, prefix = 'statemap-tagbox-tag-';

	if (g_tagsel) {
		elem = g_svgDoc.getElementById(prefix + g_tagsel.tag);
		elem.classList.remove(prefix + 'highlighted');

		if (g_tagsel.tag == tag) {
			g_tagsel = undefined;
			stateselUpdate();
			return;
		}
	}

	elem = g_svgDoc.getElementById(prefix + tag);
	elem.classList.add(prefix + 'highlighted');
	g_tagsel = { tag: tag };
	stateselUpdate();
};

var stateselClearTagbox = function ()
{
	var tagbox = g_svgDoc.getElementById('statemap-tagbox'), elem;

	if (!tagbox)
		return;

	while (tagbox.childNodes.length > 0)
		tagbox.removeChild(tagbox.childNodes[0]);

	elem = g_svgDoc.getElementById('statemap-tagbox-select');

	while (elem.childNodes.length > 0)
		elem.removeChild(elem.childNodes[0]);
};

var stateselSelect = function (state)
{
	var legend = g_svgDoc.getElementById('statemap-legend-' + state);
	var tags = {};
	var i, t;
	var lmargin = 20;
	var offset = globals.lmargin + globals.pixelWidth;

	legend.classList.add('statemap-legend-highlighted');
	stateselClearTagbox();

	t = 'tags for ' + globals.states[state].name;

	var tagbox = g_svgDoc.getElementById('statemap-tagbox');
	var x = offset + lmargin;
	var y = globals.tmargin;
	var x2 = x + (globals.tagWidth - lmargin);

	var text = g_svgDoc.createElementNS(tagbox.namespaceURI, 'text');
	text.classList.add('statemap-tagbox-header');
	text.classList.add('sansserif');

	text.appendChild(g_svgDoc.createTextNode(t));
	text.setAttributeNS(null, 'x', x);
	text.setAttributeNS(null, 'y', y);
	tagbox.appendChild(text);
	y += 10;

	var line = g_svgDoc.createElementNS(tagbox.namespaceURI, 'line');
	line.classList.add('statemap-tagbox-header-line');
	line.x1.baseVal.value = x - 2;
	line.y1.baseVal.value = y;
	line.x2.baseVal.value = x2;
	line.y2.baseVal.value = y;
	tagbox.appendChild(line);
	y += 20;

	/*
	 * Now add text for each possible tag for this state.
	 */
	for (i = 0; i < globals.tags.length; i++) {
		if (globals.tags[i].state !== state)
			continue;

		for (t in globals.tags[i]) {
			if (t == 'state' || t == 'tag')
				continue;

			tags[t] = true;
		}
	}

	tags = Object.keys(tags).sort();

	var click = function (tag) {
		return (function (evt) { stateselTagSelect(evt, tag); });
	};

	for (i = 0; i < tags.length; i++) {
		text = g_svgDoc.createElementNS(tagbox.namespaceURI, 'text');
		text.classList.add('statemap-tagbox-tag');
		text.classList.add('sansserif');
		text.id = 'statemap-tagbox-tag-' + tags[i];

		text.appendChild(g_svgDoc.createTextNode(tags[i]));
		text.setAttributeNS(null, 'x', x);
		text.setAttributeNS(null, 'y', y);
		text.addEventListener('click', click(tags[i]));

		tagbox.appendChild(text);
		y += 18;
	}

	g_statesel = { state: state, x: x, y: y, x2: x2 };
	stateselUpdate();
};

var stateselClear = function ()
{
	var state, legend;

	if (g_statesel == undefined)
		return (-1);

	state = g_statesel.state;
	legend = g_svgDoc.getElementById('statemap-legend-' + state);
	legend.classList.remove('statemap-legend-highlighted');

	stateselClearTagbox();
	g_statesel = undefined;
	g_tagsel = undefined;

	return (state);
};

/*
 * All of the following *click() functions are added at the time of statemap
 * generation.
 */
var legendclick = function (evt, state)
{
	if (globals.notags || stateselClear() == state)
		return;

	stateselSelect(state);
	stateselUpdate();
};

var mapclick = function (evt, idx)
{
	var x = evt.clientX - g_offset;

	timebarRemove(g_timebar);
	g_timebar = timebarCreate(x);

	statebarRemove(g_statebar);
	g_statebar = statebarCreate(evt.target, idx);

	stateselUpdate();
};

var panclick = function (dx, dy)
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
	stateselUpdate();
};

var zoomclick = function (scale)
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
	stateselUpdate();
};
