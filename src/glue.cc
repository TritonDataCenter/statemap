#include <node.h>
#include <string>
#include <strings.h>
#include "statemap.h"

namespace glue {

using v8::FunctionCallbackInfo;
using v8::Isolate;
using v8::Local;
using v8::Object;
using v8::String;
using v8::Value;

static int
ingestEmitEntity(const FunctionCallbackInfo<Value>& args,
    statemap_t *statemap, statemap_entity_t *entity)
{
	statemap_rect_t *rect;
	Isolate *isolate = args.GetIsolate();

	Local<v8::Function> cb = Local<v8::Function>::Cast(args[1]);
	const unsigned argc = 1;

	Local<String> start = String::NewFromUtf8(isolate, "time");
	Local<String> duration = String::NewFromUtf8(isolate, "duration");
	Local<String> states = String::NewFromUtf8(isolate, "states");
	Local<String> ent = String::NewFromUtf8(isolate, "entity");
	Local<String> name = String::NewFromUtf8(isolate, entity->sme_name);

	if (entity->sme_description != NULL) {
		Local<Object> obj = Object::New(isolate);

		obj->Set(ent, name);
		obj->Set(String::NewFromUtf8(isolate, "description"),
		    String::NewFromUtf8(isolate, entity->sme_description));

		Local<Value> argv[argc] = { obj };
		cb->Call(Null(isolate), argc, argv);
	}

	for (rect = entity->sme_first; rect != NULL; rect = rect->smr_next) {
		Local<Object> obj = Object::New(isolate);
		Local<v8::Array> arr = v8::Array::New(isolate);

		int i;

		obj->Set(ent, name);
		obj->Set(states, arr);
		obj->Set(start, v8::Number::New(isolate, rect->smr_start));
		obj->Set(duration,
		    v8::Number::New(isolate, rect->smr_duration));

		for (i = 0; i < statemap->sm_nstates; i++) {
			arr->Set(v8::Number::New(isolate, i),
			    v8::Number::New(isolate, rect->smr_states[i]));
		}
		
		Local<Value> argv[argc] = { obj };
		cb->Call(Null(isolate), argc, argv);
	}

	return (0);
}

#define LOADCONFIG_INTFIELD(field) \
	val = obj->Get(String::NewFromUtf8(isolate, #field)); \
	if (!(val->IsUndefined())) { \
		if (!val->IsNumber()) { \
			isolate->ThrowException(v8::Exception::Error( \
			    String::NewFromUtf8(isolate, "expected config " \
			    "field " #field " to be a number"))); \
			return (-1); \
		} \
		config->smc_##field = val->IntegerValue(); \
	}

static int
loadConfig(Isolate *isolate, statemap_config_t *config, Local<Object> obj)
{
	Local<Value> val;

	LOADCONFIG_INTFIELD(maxrect);
	LOADCONFIG_INTFIELD(begin);
	LOADCONFIG_INTFIELD(end);

	return (0);
}

#undef LOADCONFIG_INTFIELD

/* 
 * We expect three arguments: a filename, a callback, and an optional
 * configuration object.
 */
void
ingest(const FunctionCallbackInfo<Value>& args)
{
	Isolate *isolate = args.GetIsolate();
	statemap_config_t config;
	statemap_entity_t *entity;
	statemap_t *statemap;

	bzero(&config, sizeof (config));

	if (args.Length() == 3) {
		if (!args[2]->IsObject()) {
			isolate->ThrowException(v8::Exception::Error(
			    String::NewFromUtf8(isolate, "expected config "
			    "object")));
		}

		if (loadConfig(isolate, &config, args[2]->ToObject()) != 0)
			return;
	}

	if ((statemap = statemap_create(&config)) == NULL) {
		isolate->ThrowException(v8::Exception::Error(
		    String::NewFromUtf8(isolate, "could not create statemap")));
		return;
	}

	if (args.Length() == 0 || !args[0]->IsString()) {
		isolate->ThrowException(v8::Exception::TypeError(
		    String::NewFromUtf8(isolate, "expected file name")));
		statemap_destroy(statemap);
		return;
	}

	if (args.Length() == 1 || !args[1]->IsFunction()) {
		isolate->ThrowException(v8::Exception::TypeError(
		    String::NewFromUtf8(isolate, "expected callback")));
		statemap_destroy(statemap);
		return;
	}

	v8::String::Utf8Value val(args[0]->ToString());
	std::string str(*val);

	if (statemap_ingest(statemap, str.c_str()) != 0) {
		isolate->ThrowException(v8::Exception::Error(
		    String::NewFromUtf8(isolate, statemap_errmsg(statemap))));
		statemap_destroy(statemap);
		return;
	}

	for (entity = statemap->sm_entities; entity != NULL;
	    entity = entity->sme_next) {
		if (ingestEmitEntity(args, statemap, entity) != 0)
			break;
	}
	
	args.GetReturnValue().Set(v8::Number::New(isolate,
	    statemap->sm_ncoalesced));

	statemap_destroy(statemap);
}

void
init(Local<Object> exports)
{
	NODE_SET_METHOD(exports, "ingest", ingest);
}

NODE_MODULE(NODE_GYP_MODULE_NAME, init)

}  // namespace glue
