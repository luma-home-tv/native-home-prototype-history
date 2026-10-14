/* Narrow SDL ABI adapter. Built against each target's actual headers: webOS adds fields
 * to keyboard/mouse events, so Rust must not reinterpret the desktop SDL_Event layout. */
#include <SDL.h>
#include <stdint.h>
#include <string.h>
typedef struct {uint32_t kind, key, repeat, source; int32_t scan, x, y, wheel;} HomeEvent;
_Static_assert(sizeof(HomeEvent)==32,"HomeEvent ABI");
int native_home_poll(HomeEvent *out) {
    SDL_Event e;
    while(SDL_PollEvent(&e)) {
        memset(out,0,sizeof(*out));
        if(e.type==SDL_QUIT) {out->kind=1;return 1;}
        if(e.type==SDL_WINDOWEVENT) {
            if(e.window.event==SDL_WINDOWEVENT_CLOSE) {out->kind=1;return 1;}
            if(e.window.event==SDL_WINDOWEVENT_FOCUS_LOST) {out->kind=7;return 1;}
            if(e.window.event==SDL_WINDOWEVENT_HIDDEN||e.window.event==SDL_WINDOWEVENT_MINIMIZED) {out->kind=8;return 1;}
            if(e.window.event==SDL_WINDOWEVENT_FOCUS_GAINED||e.window.event==SDL_WINDOWEVENT_SHOWN||e.window.event==SDL_WINDOWEVENT_RESTORED) {out->kind=9;return 1;}
        }
        if(e.type==SDL_KEYDOWN||e.type==SDL_KEYUP) {
            out->kind=e.type==SDL_KEYDOWN?2:3;out->repeat=e.key.repeat;out->scan=e.key.keysym.scancode;
#ifdef NATIVE_HOME_WEBOS
            out->source=e.key.inputSource;
#endif
            switch(e.key.keysym.sym) {
                case SDLK_UP:out->key=1;break;case SDLK_DOWN:out->key=2;break;
                case SDLK_LEFT:out->key=3;break;case SDLK_RIGHT:out->key=4;break;
                case SDLK_RETURN:case SDLK_KP_ENTER:out->key=5;break;
                case SDLK_ESCAPE:out->key=6;break;
                default:break;
            }
#ifdef NATIVE_HOME_WEBOS
            /* Observed on this webOS24 TV: remote OK 77, Back 482. Do not consume Home,
             * power, volume, settings or channel keys, nor guess unobserved LG scancodes. */
            if(out->scan==77)out->key=5;
            if(out->scan==482)out->key=6;
#endif
            if(out->key)return 1;
        }
        if(e.type==SDL_MOUSEMOTION) {out->kind=4;out->x=e.motion.x;out->y=e.motion.y;return 1;}
        if(e.type==SDL_MOUSEBUTTONUP&&e.button.button==SDL_BUTTON_LEFT) {out->kind=5;out->x=e.button.x;out->y=e.button.y;return 1;}
        if(e.type==SDL_MOUSEWHEEL) {out->kind=6;out->wheel=e.wheel.y*(e.wheel.direction==SDL_MOUSEWHEEL_FLIPPED?-1:1);return 1;}
    }
    return 0;
}
