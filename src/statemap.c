/*
 * Copyright 2018 Joyent, Inc.
 */

#include <sys/stat.h>
#include <stdio.h>
#include <stdarg.h>
#include <errno.h>
#include <string.h>
#include <fcntl.h>
#include <stdlib.h>
#include <unistd.h>
#include <sys/mman.h>
#include <ctype.h>
#include <assert.h>

#include "statemap.h"
#include "./jsmn/jsmn.h"

int
assfail(const char *expr, const char *file, int line)
{
	(void) fprintf(stderr, "Assertion failed: %s in file %s, line %d\n",
	    expr, file, line);

	assert(0);
}

static void
statemap_error(statemap_t *statemap, char *fmt, ...)
{
	va_list ap;
	int error = errno;
	int rem, len;
	char *buf = statemap->sm_errmsg;
	size_t size = sizeof (statemap->sm_errmsg);

	va_start(ap, fmt);

	if (fmt[0] == '?') {
		/*
		 * A leading question mark denotes that the error message
		 * should be prepended with the line that is failing.
		 */
		int len = snprintf(buf, size, "illegal datum on line %ld: ",
		    statemap->sm_line);

		(void) vsnprintf(buf + len, size - len, fmt + 1, ap);
		return;
	}

	(void) vsnprintf(buf, size, fmt, ap);

	rem = size - (len = strlen(buf));

	if (fmt[strlen(fmt) - 1] == '\n') {
		/*
		 * A trailing newline denotes that we should not append
		 * strerror -- but we still want to zap the newline.
		 */
		buf[strlen(buf) - 1] = '\0';
		return;
	}

	(void) snprintf(&buf[len], rem, ": %s", strerror(error));
}

const char *
statemap_errmsg(statemap_t *statemap)
{
	return (statemap->sm_errmsg);
}

static const char *
statemap_tokstr(jsmntok_t *tok, char *base)
{
	static char buf[256];
	int i;
	size_t j = 0;

	for (i = tok->start; i < tok->end && j < sizeof (buf) - 1; i++, j++)
		buf[j] = base[i];

	buf[j] = '\0';

	return (buf);
}

static int
statemap_tokstrcmp(jsmntok_t *tok, char *base, char *cmp)
{
	int i, j = 0;

	for (i = tok->start; i < tok->end; i++, j++) {
		if (base[i] != cmp[j])
			return (-1);
	}

	return (cmp[j] == '\0' ? 0 : -1);
}

/*
 * Hash the string associated with a token.  For Old Times' sake, we use
 * Bob Jenkins' One-at-a-time algorithm, a simple quick algorithm that has
 * no known funnels.
 */
static int
statemap_tokhash(jsmntok_t *tok, char *base)
{
	int hashval = 0, i;

	for (i = tok->start; i < tok->end; i++) {
		hashval += base[i];
		hashval += (hashval << 10);
		hashval ^= (hashval >> 6);
	}

	return (hashval);
}

static long long
statemap_tokint(jsmntok_t *tok, char *base)
{
	long long v = 0, place = 1;
	int i;

	if (tok->type != JSMN_PRIMITIVE && tok->type != JSMN_STRING)
		return (-1);

	for (i = tok->end - 1; i >= tok->start; i--, place *= 10) {
		if (base[i] < '0' || base[i] > '9')
			return (-1);

		v += (base[i] - '0') * place;
	}

	return (v);
}

#ifdef DEBUG
static void
statemap_tokdump(jsmntok_t *tok, char *base, int ntok)
{
	int i;

	for (i = 0; i < ntok; i++) {
		printf("[%d]: type=%d (%s) start=%d end=%d (\"%s\") "
		    "size=%d parent=%d\n",
		    i, tok[i].type,
		    tok[i].type == JSMN_UNDEFINED ? "UNDEFINED" :
		    tok[i].type == JSMN_OBJECT ? "OBJECT" :
		    tok[i].type == JSMN_ARRAY ? "ARRAY" :
		    tok[i].type == JSMN_STRING ? "STRING" :
		    tok[i].type == JSMN_PRIMITIVE ? "PRIMITIVE" : "???",
		    tok[i].start, tok[i].end,
		    statemap_tokstr(&tok[i], base),
		    tok[i].size, tok[i].parent);
	}
}
#endif

/*
 * Finds the start of the next concatenated JSON blob.  JSON blobs should
 * be delimited with whitespace.
 */
static char *
statemap_json_start(statemap_t *statemap, char *ptr, const char *lim)
{
	char c;

	while (ptr < lim && (c = *ptr) != '{') {
		if (c == '\n')
			statemap->sm_line++;

		if (!isspace(c)) {
			(void) statemap_error(statemap, "line %d: illegal JSON"
			    " delimiter (\"%c\")\n", statemap->sm_line, c);
			return (NULL);
		}

		ptr++;
	}

	return (ptr);
}

/*
 * For a given JSON blob, returns a pointer to the character immediately beyond
 * the blob.
 */
static char *
statemap_json_end(statemap_t *statemap, char *ptr, const char *lim)
{
	int notinstring = 1, backslashed = 0;
	int line = statemap->sm_line;
	int start = line;
	int depth = 1;
	char c;

	assert(*ptr == '{');
	ptr++;

	while (ptr < lim) {
		c = *ptr++;

		if (c == '\n')
			line++;

		if (backslashed) {
			backslashed = 0;
			continue;
		}

		switch (c) {
		case '"':
			notinstring ^= 1;
			break;

		case '\\':
			backslashed = 1;
			break;

		case '{':
			depth += notinstring;
			break;

		case '}':
			depth -= notinstring;

			if (depth == 0) {
				statemap->sm_line = line;
				return (ptr);
			}

			break;
		}
	}

	if (ptr == lim) {
		statemap_error(statemap, "JSON payload starting at line "
		    "%d is not terminated\n", start);
		return (NULL);
	}

	return (ptr);
}

statemap_entity_t *
statemap_entity_lookup(statemap_t *statemap, jsmntok_t *tok, char *base)
{
	int hash = statemap_tokhash(tok, base);
	statemap_entity_t *entity, **ep;

	ep = &statemap->sm_hash[hash % STATEMAP_ENTITY_HASHSIZE];

	for (entity = *ep; entity != NULL; entity = entity->sme_hashnext) {
		if (statemap_tokstrcmp(tok, base, entity->sme_name) == 0)
			return (entity);
	}

	/*
	 * We don't have our entity in the hash table; create one.
	 */
	if ((entity = malloc(sizeof (statemap_entity_t))) == NULL) {
		statemap_error(statemap, "failed to allocated entity for "
		    "\"%s\"", statemap_tokstr(tok, base));
		return (NULL);
	}

	bzero(entity, sizeof (statemap_entity_t));
	entity->sme_name = strndup(&base[tok->start], tok->end - tok->start);
	entity->sme_start = -1;

	if (entity->sme_name == NULL) {
		statemap_error(statemap, "failed to allocated name for "
		    "entity \"%s\"", statemap_tokstr(tok, base));
		free(entity);
		return (NULL);
	}

	entity->sme_hashnext = *ep;
	*ep = entity;

	entity->sme_next = statemap->sm_entities;
	statemap->sm_entities = entity;

	return (entity);
}

int
statemap_rect_cmp(const void *l, const void *r)
{
	const statemap_rect_t *lhs = l, *rhs = r;

	if (lhs->smr_weight < rhs->smr_weight)
		return (-1);

	if (lhs->smr_weight > rhs->smr_weight)
		return (1);

	if (lhs->smr_duration < rhs->smr_duration)
		return (-1);

	if (lhs->smr_duration > rhs->smr_duration)
		return (1);

	if (lhs->smr_start < rhs->smr_start)
		return (-1);

	if (lhs->smr_start > rhs->smr_start)
		return (1);

	return (strcmp(lhs->smr_entity->sme_name, rhs->smr_entity->sme_name));
}

void
statemap_rect_add(statemap_rect_t *rect, avl_tree_t *rects)
{
	long long weight = rect->smr_duration;

	if (rect->smr_prev != NULL)
		weight += rect->smr_prev->smr_duration;

	if (rect->smr_next != NULL)
		weight += rect->smr_next->smr_duration;

	rect->smr_weight = weight;
	avl_add(rects, rect);
}

void
statemap_rect_update(statemap_rect_t *rect, avl_tree_t *rects)
{
	long long weight = rect->smr_duration;

	if (rect == NULL)
		return;

	if (rect->smr_prev != NULL)
		weight += rect->smr_prev->smr_duration;

	if (rect->smr_next != NULL)
		weight += rect->smr_next->smr_duration;

	if (weight > rect->smr_weight) {
		rect->smr_weight = weight;
		avl_update_gt(rects, rect);
		return;
	}

	if (weight < rect->smr_weight) {
		rect->smr_weight = weight;
		avl_update_lt(rects, rect);
		return;
	}
}

statemap_t *
statemap_create(statemap_config_t *config)
{
	statemap_t *statemap = malloc(sizeof (statemap_t));

	if (statemap == NULL)
		return (NULL);

	bzero(statemap, sizeof (statemap_t));

	if (config != NULL)
		bcopy(config, &statemap->sm_config, sizeof (statemap_config_t));

	if (statemap->sm_config.smc_maxrect == 0)
		statemap->sm_config.smc_maxrect = STATEMAP_CONFIG_MAXRECT;

	avl_create(&statemap->sm_rects, statemap_rect_cmp,
	    sizeof (statemap_rect_t), offsetof(statemap_rect_t, smr_node));

	return (statemap);
}

void
statemap_destroy(statemap_t *statemap)
{
	statemap_entity_t *entity, *next;
	statemap_rect_t *rect, *nextr;

	for (entity = statemap->sm_entities; entity != NULL; entity = next) {
		for (rect = entity->sme_first; rect != NULL; rect = nextr) {
			nextr = rect->smr_next;
			free(rect);
		}

		next = entity->sme_next;
		free(entity);
	}

	free(statemap);
}

int
statemap_ingest_metadata(statemap_t *statemap, char *base, long len)
{
	jsmn_parser parser;
	jsmntok_t *tok;
	int ntok, maxtok, i, j, states, nstates = 0;
	long long val;
	int *stateval;

	jsmn_init(&parser);

	if (len > STATEMAP_METADATA_MAX) {
		statemap_error(statemap, "size of metadata (%d bytes) exceeds "
		    "maximum (%d bytes)\n", len, STATEMAP_METADATA_MAX);
		return (-1);
	}

	/*
	 * The maximum number of tokens is generated with an encapsulated
	 * empty, unnamed object -- which generates a token for every two
	 * characters.
	 */
	maxtok = len / 2;
	tok = alloca(maxtok * sizeof (jsmntok_t));
	ntok = jsmn_parse(&parser, base, len, tok, maxtok);

	assert(ntok != JSMN_ERROR_NOMEM);

	if (ntok == JSMN_ERROR_INVAL || ntok == JSMN_ERROR_PART) {
		/*
		 * Yes, this is an annoyingly spartan error message -- but
		 * we also don't expect users to see it because we expect the
		 * metadata to be more thoroughly validated before we actually
		 * attempt to ingest it.
		 */
		statemap_error(statemap, "malformed metadata\n");
		return (-1);
	}

	/*
	 * For our purposes, we only really care about the values for the
	 * states.
	 */
	for (i = 0; i < ntok; i++) {
		if (tok[i].parent != 0 || tok[i].type != JSMN_STRING)
			continue;

		if (strncmp(&base[tok[i].start], STATEMAP_METADATA_STATES,
		    tok[i].end - tok[i].start) == 0) {
			break;
		}
	}

	if (++i >= ntok) {
		statemap_error(statemap, "missing \"states\" in metadata\n");
		return (-1);
	}

	if (tok[i].type != JSMN_OBJECT || tok[i].parent != i - 1) {
		statemap_error(statemap, "invalid metadata: \"states\" "
		    "must be an object\n");
		return (-1);
	}

	/*
	 * Now iterate over the states to validate that each has a "value"
	 * member and that those values don't overlap and don't exceed the
	 * number of states.
	 */
	states = i++;

	for (j = i; j < ntok; j++) {
		if (tok[j].parent == states)
			nstates++;
	}

	assert(nstates < maxtok);
	stateval = alloca(nstates * sizeof (int));
	bzero(stateval, nstates * sizeof (int));

	for (;;) {
		while (i < ntok && tok[i].parent != states)
			i++;

		if (i == ntok)
			break;

		/*
		 * We expect the pattern here to be a string (the name of the
		 * state) followed by an object that contains at least a "value"
		 * member.
		 */
		if (tok[i].type != JSMN_STRING || i + 1 == ntok ||
		    tok[i + 1].type != JSMN_OBJECT) {
			statemap_error(statemap, "\"states\" members "
			    "must be objects");
		}

		for (j = i + 2; j < ntok; j++) {
			if (tok[j].parent != i + 1)
				continue;

			if (tok[j].type != JSMN_STRING)
				continue;

			if (strncmp(&base[tok[j].start],
			    STATEMAP_METADATA_STATESVALUE,
			    tok[j].end - tok[j].start) == 0) {
				break;
			}
		}

		if (++j >= ntok) {
			statemap_error(statemap, "state \"%s\" is missing a "
			    "value field\n", statemap_tokstr(&tok[i], base));
			return (-1);
		}

		/*
		 * We have our value!
		 */
		if (tok[j].parent != j - 1 || tok[j].type != JSMN_PRIMITIVE ||
		    (val = statemap_tokint(&tok[j], base)) == -1) {
			statemap_error(statemap, "\"value\" member "
			    "for state \"%s\" is not an integer\n",
			    statemap_tokstr(&tok[i], base));
			return (-1);
		}

		if (val >= nstates) {
			statemap_error(statemap, "\"value\" member "
			    "for state \"%s\" exceeds maximum (%d)\n",
			    statemap_tokstr(&tok[i], base), nstates - 1);
			return (-1);
		}

		if (stateval[(int)val] != 0) {
			char buf[256];
			const char *c;

			c = statemap_tokstr(&tok[stateval[(int)val]], base);
			(void) strncpy(buf, c, sizeof (buf));

			statemap_error(statemap, "\"value\" for state \"%s\""
			    " (%lld) conflicts with that of state \"%s\"\n",
			    statemap_tokstr(&tok[i], base), val, buf);
			return (-1);
		}

		stateval[(int)val] = i;
		i = j;
	}

	statemap->sm_nstates = nstates;
	statemap->sm_rectsize = sizeof (statemap_rect_t) +
	    ((nstates - 1) * sizeof (long long));

	return (0);
}

static int
statemap_ingest_newrect(statemap_t *statemap,
    statemap_entity_t *entity, long long time)
{
	statemap_rect_t *rect;
	statemap_rect_t *victim, *survivor, *left, *right;
	avl_tree_t *rects = &statemap->sm_rects;
	int i;

	/*
	 * We have a new rectangle!  Grab one off the freelist (if there is
	 * one) and fill it in.
	 */
	if (statemap->sm_freerect) {
		rect = statemap->sm_freerect;
		statemap->sm_freerect = rect->smr_next;
	} else {
		if ((rect = malloc(statemap->sm_rectsize)) == NULL) {
			statemap_error(statemap, "couldn't allocate new rect");
			return (-1);
		}
	}

	bzero(rect, statemap->sm_rectsize);
	rect->smr_start = entity->sme_start;
	rect->smr_duration = time - entity->sme_start;
	rect->smr_states[entity->sme_state] = rect->smr_duration;
	rect->smr_entity = entity;

	/*
	 * And now link it on to the list of rectangles for this entity.
	 */
	rect->smr_prev = entity->sme_last;

	if (entity->sme_first == NULL) {
		entity->sme_first = rect;
	} else {
		entity->sme_last->smr_next = rect;

		/*
		 * Update the weight of our neighbor.
		 */
		statemap_rect_update(entity->sme_last, rects);
	}

	entity->sme_last = rect;
	statemap_rect_add(rect, rects);

	/*
	 * If we haven't yet reached our maximum number of rectangles, we're
	 * done with this datum!
	 */
	if (avl_numnodes(rects) <= (ulong_t)statemap->sm_config.smc_maxrect)
		return (0);

	/*
	 * If we're here, we need to coalesce our lightest weight rectangle
	 * with one of its neighbors.
	 */
	for (victim = avl_first(rects); victim != NULL;
	    victim = avl_walk(rects, victim, AVL_AFTER)) {
		if (victim->smr_prev != NULL || victim->smr_next != NULL)
			break;

		assert(victim->smr_entity->sme_first == victim);
		assert(victim->smr_entity->sme_last == victim);
	}

	if (victim == NULL) {
		/*
		 * Oddly, we have nothing to coalesce with -- presumably
		 * because we have many entities or a low maximum.  Either
		 * way, we have nothing else to do.
		 */
		return (0);
	}

	/*
	 * We have a victim -- now let's figure out if we want to coalesce
	 * with the rectangle to its right or to its left.
	 */
	if (victim->smr_prev == NULL) {
		left = victim;
		right = survivor = victim->smr_next;
	} else if (victim->smr_next == NULL) {
		left = survivor = victim->smr_prev;
		right = victim;
	} else if (victim->smr_prev->smr_duration <
	    victim->smr_next->smr_duration) {
		left = survivor = victim->smr_prev;
		right = victim;
	} else {
		left = victim;
		right = survivor = victim->smr_next;
	}

	assert(survivor->smr_entity == victim->smr_entity);
	survivor->smr_duration += victim->smr_duration;

	/*
	 * Add our victim's state durations into our surivivor's.
	 */
	for (i = 0; i < statemap->sm_nstates; i++) {
		assert(victim->smr_states[i] <= victim->smr_duration);
		survivor->smr_states[i] += victim->smr_states[i];
		assert(survivor->smr_states[i] <= survivor->smr_duration);
	}

	/*
	 * Now we need to actually eliminate the victim, updating the survivor
	 * and its neighbors appropriately.
	 */
	if (victim == left) {
		survivor->smr_start = victim->smr_start;

		if ((survivor->smr_prev = victim->smr_prev) == NULL) {
			assert(victim->smr_entity->sme_first == victim);
			victim->smr_entity->sme_first = survivor;
		} else {
			survivor->smr_prev->smr_next = survivor;
		}
	} else {
		if ((survivor->smr_next = victim->smr_next) == NULL) {
			assert(victim->smr_entity->sme_last == victim);
			victim->smr_entity->sme_last = survivor;
		} else {
			survivor->smr_next->smr_prev = survivor;
		}
	}

	/*
	 * Finally, remove the victim from the AVL tree (and put it on the
	 * statemap's freelist), update the survivor with the new (larger)
	 * duration, and then update the weight of the survivor's neighbors.
	 */
	avl_remove(rects, victim);
	victim->smr_next = statemap->sm_freerect;
	statemap->sm_freerect = victim;

	statemap_rect_update(survivor->smr_prev, rects);
	statemap_rect_update(survivor, rects);
	statemap_rect_update(survivor->smr_next, rects);
	statemap->sm_ncoalesced++;

	return (0);
}

/*
 * Macro to check if a token matches a field we're looking for.  This
 * deliberately uses strncmp() instead of statemap_tokstrcmp() to allow for
 * partial matches.
 */
#define	STATEMAP_INGEST_CHECKTOK(field, t) \
	if (strncmp(field, str, len) == 0) { \
		if ((t) != NULL) { \
			statemap_error(statemap, "datum on line %d " \
			    "contains duplicate \"%s\"", statemap->sm_line, \
			    field); \
			return (-1); \
		} \
		t = &tok[++i]; \
		continue; \
	}

int
statemap_ingest_data(statemap_t *statemap, char *base, long len)
{
	jsmn_parser parser;
	jsmntok_t tok[10];
	jsmntok_t *timetok = NULL, *entitytok = NULL, *statetok = NULL;
	jsmntok_t *eventtok = NULL, *descrtok = NULL;
	statemap_entity_t *entity;
	int ntok, i;
	long long time;
	int state;

	jsmn_init(&parser);

	ntok = jsmn_parse(&parser, base, len, tok, 10);

	if (ntok == JSMN_ERROR_NOMEM) {
		statemap_error(statemap, "JSON data at line %d contains "
		    "too many fields\n", statemap->sm_line);
		return (-1);
	}

	if (ntok == JSMN_ERROR_INVAL || ntok == JSMN_ERROR_PART) {
		statemap_error(statemap, "malformed JSON data on line %d\n",
		    statemap->sm_line);
		return (-1);
	}

	for (i = 0; i < ntok; i++) {
		char *str;
		int len;

		if (tok[i].parent != 0 || tok[i].type != JSMN_STRING)
			continue;

		str = &base[tok[i].start];
		len = tok[i].end - tok[i].start;

		if (i == ntok - 1 || tok[i + 1].parent != i) {
			statemap_error(statemap, "malformed JSON data on "
			    "line %d: missing value for field \"%s\"\n",
			    statemap->sm_line, statemap_tokstr(&tok[i], base));
			return (-1);
		}

		STATEMAP_INGEST_CHECKTOK(STATEMAP_DATA_ENTITY, entitytok)
		STATEMAP_INGEST_CHECKTOK(STATEMAP_DATA_TIME, timetok)
		STATEMAP_INGEST_CHECKTOK(STATEMAP_DATA_STATE, statetok)
		STATEMAP_INGEST_CHECKTOK(STATEMAP_DATA_EVENT, eventtok)
		STATEMAP_INGEST_CHECKTOK(STATEMAP_DATA_DESCRIPTION, descrtok)
	}

	/*
	 * Every datum must have an entity.
	 */
	if (entitytok == NULL) {
		statemap_error(statemap, "illegal datum on line %d: missing "
		    "\"%s\" field\n", statemap->sm_line, STATEMAP_DATA_ENTITY);
		return (-1);
	}

	/*
	 * Look up our entity, creating one if it doesn't exist.
	 */
	entity = statemap_entity_lookup(statemap, entitytok, base);

	if (entity == NULL)
		return (-1);

	if (timetok == NULL) {
		/*
		 * The only legal datum that lacks a "time" field is one
		 * that provides additional entity description.
		 */
		if (descrtok == NULL) {
			statemap_error(statemap, "?missing \"%s\" or \"%s\"",
			    STATEMAP_DATA_TIME, STATEMAP_DATA_DESCRIPTION);
			return (-1);
		}

		if (entity->sme_description != NULL)
			free(entity->sme_description);

		if ((entity->sme_description = strndup(&base[descrtok->start],
		    descrtok->end - descrtok->start)) == NULL) {
			statemap_error(statemap, "failed to allocate "
			    "description for entity \"%s\"", entity->sme_name);
			return (-1);
		}

		return (0);
	}

	if (statetok == NULL) {
		if (eventtok != NULL) {
			/*
			 * Right now, we don't do anything with events -- but
			 * the intent is to be able to render these in the
			 * statemap, so we also don't reject them.
			 */
			statemap->sm_nevents++;
			return (0);
		}

		statemap_error(statemap,
		    "?missing \"%s\" field", STATEMAP_DATA_STATE);
		return (-1);
	}

	if ((time = statemap_tokint(timetok, base)) == -1) {
		statemap_error(statemap, "?\"%s\" is not a positive "
		    "integer", STATEMAP_DATA_TIME);
		return (-1);
	}

	/*
	 * If the time of this datum is after our specified end time, we
	 * have nothing further to do to process it.
	 */
	if (statemap->sm_config.smc_end && time > statemap->sm_config.smc_end)
		return (0);

	if ((state = statemap_tokint(statetok, base)) == -1 ||
	    state >= statemap->sm_nstates) {
		statemap_error(statemap, "?illegal state value");
		return (-1);
	}

	if (entity->sme_start < 0) {
		/*
		 * This is the first state we have seen for this entity; we
		 * don't have anything to do other than record our state and
		 * when it started.
		 */
		entity->sme_start = time;
		entity->sme_state = state;
		return (0);
	}

	if (time < entity->sme_start) {
		statemap_error(statemap, "?time %lld is out of order with "
		    "respect to prior time %lld\n", time, entity->sme_start);
		return (-1);
	}

	if (time == entity->sme_start) {
		statemap->sm_nelisions++;
		entity->sme_state = state;
		return (0);
	}

	if (time > statemap->sm_config.smc_begin) {
		/*
		 * We can now create a new rectangle for this entity's past
		 * state.
		 */
		if (entity->sme_start < statemap->sm_config.smc_begin)
			entity->sme_start = statemap->sm_config.smc_begin;

		if (statemap_ingest_newrect(statemap, entity, time) != 0)
			return (-1);
	}

	/*
	 * And now reset our entity's start time and state.
	 */
	entity->sme_start = time;
	entity->sme_state = state;

	return (0);
}

static int
statemap_ingest_end(statemap_t *statemap, long long end)
{
	statemap_entity_t *entity;

	if (end == 0) {
		/*
		 * If we weren't given an ending time, take a lap through all
		 * of our entities to find the one with the latest time.
		 */
		for (entity = statemap->sm_entities; entity != NULL;
		    entity = entity->sme_next) {
			if (entity->sme_start > end)
				end = entity->sme_start;
		}
	}

	for (entity = statemap->sm_entities; entity != NULL;
	    entity = entity->sme_next) {
		if (entity->sme_start == -1 || entity->sme_start >= end)
			continue;

		if (statemap_ingest_newrect(statemap, entity, end) != 0)
			return (-1);
	}

	return (0);
}

int
statemap_ingest(statemap_t *statemap, const char *filename)
{
	struct stat buf;
	void *addr = NULL;
	char *ptr, *lim, *end;
	int fd = -1, rval = -1;

	if (stat(filename, &buf) != 0) {
		statemap_error(statemap, "failed to stat %s", filename);
		goto err;
	}

	if ((fd = open(filename, O_RDONLY)) == -1) {
		statemap_error(statemap, "failed to open %s", filename);
		goto err;
	}

	addr = mmap(NULL, buf.st_size, PROT_READ, MAP_SHARED, fd, 0);

	if (addr == (void *)-1) {
		statemap_error(statemap, "failed to map %s", filename);
		goto err;
	}

	ptr = addr;
	lim = addr + buf.st_size;
	statemap->sm_line = 1;

	if ((ptr = statemap_json_start(statemap, ptr, lim)) == NULL)
		goto err;

	if (ptr == lim) {
		/*
		 * There isn't a metadata payload here at all.
		 */
		statemap_error(statemap, "missing metadata payload\n");
		goto err;
	}

	if ((end = statemap_json_end(statemap, ptr, lim)) == NULL)
		goto err;

	if (statemap_ingest_metadata(statemap, ptr, end - ptr) != 0)
		goto err;

	/*
	 * Now it's time to actually rip through the data!
	 */
	ptr = end;

	while ((ptr = statemap_json_start(statemap, ptr, lim)) != NULL) {
		if (ptr == lim)
			break;

		if ((end = statemap_json_end(statemap, ptr, lim)) == NULL)
			goto err;

		if (statemap_ingest_data(statemap, ptr, end - ptr) != 0)
			goto err;

		ptr = end;
	}

	if (statemap_ingest_end(statemap, statemap->sm_config.smc_end) != 0)
		goto err;

	rval = 0;
err:
	if (addr != (void *)-1)
		(void) munmap(addr, buf.st_size);

	if (fd != -1)
		close(fd);

	return (rval);
}
