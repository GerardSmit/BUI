/* Damage: an immediate-mode host (the Rust bindings) puts every canvas item again and
 * places every widget again each frame. An unchanged frame must redraw nothing, a changed
 * item only around itself, and the incremental frames must equal a full redraw of the same
 * items (no stale pixels). */

#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

#include "BUI_kit.h"
#include "GPU_raster.h"

static int g_fail = 0;

#define CHECK(cond) \
  do { \
    if (!(cond)) { \
      fprintf(stderr, "%s:%d: CHECK(%s) failed\n", __FILE__, __LINE__, #cond); \
      g_fail = 1; \
    } \
  } while (0)

enum { W = 320, H = 240, MAX_ITEMS = 48 };

typedef struct Scene {
  BKCanvasItem items[MAX_ITEMS];
  float coords[MAX_ITEMS][8];
  char text[MAX_ITEMS][16];
  int n;
} Scene;

static uint32_t g_seed = 12345;

static uint32_t rnd(uint32_t n)
{
  g_seed = g_seed * 1664525u + 1013904223u;
  return (g_seed >> 8) % n;
}

static const char *g_family_a = "";

static BKImBuf *g_img[2];

static void random_item(Scene *s, int i)
{
  BKCanvasItem *it = &s->items[i];
  memset(it, 0, sizeof(*it));
  it->id = (int)rnd(1000) + 1;
  const int kinds[] = {BK_ITEM_RECT, BK_ITEM_OVAL, BK_ITEM_LINE, BK_ITEM_TEXT, BK_ITEM_ROUNDRECT,
                       BK_ITEM_IMAGE, BK_ITEM_POLYGON};
  it->type = kinds[rnd(7)];
  const float x = (float)rnd(W - 40), y = (float)rnd(H - 30);
  const float w = 4.0f + (float)rnd(60), h = 4.0f + (float)rnd(40);
  float *c = s->coords[i];
  c[0] = x;
  c[1] = y;
  c[2] = x + w;
  c[3] = y + h;
  c[4] = x;
  c[5] = y + h;
  it->coords = c;
  it->ncoords = it->type == BK_ITEM_TEXT ? 2 : (it->type == BK_ITEM_POLYGON ? 6 : 4);
  for (int k = 0; k < 4; k++) {
    it->fill[k] = (float)rnd(256) / 255.0f;
    it->outline[k] = rnd(3) == 0 ? (float)rnd(256) / 255.0f : 0.0f;
  }
  it->fill[3] = 0.4f + (float)rnd(60) / 100.0f;
  it->width = 1.0f + (float)rnd(3);
  it->radius = (float)rnd(8);
  if (it->type == BK_ITEM_TEXT) {
    snprintf(s->text[i], sizeof(s->text[i]), "tx%u", (unsigned)rnd(10000));
    it->text = s->text[i];
    it->size = 10.0f + (float)rnd(8);
    it->anchor = BK_ANCHOR_N | BK_ANCHOR_W;
    it->family = g_family_a;
  }
  if (it->type == BK_ITEM_IMAGE) {
    it->imbuf = g_img[rnd(2)];
    it->fit = BK_FIT_FILL;
  }
  if (rnd(6) == 0) {
    it->clip_on = 1;
    it->clip[0] = x + 2;
    it->clip[1] = y + 2;
    it->clip[2] = w / 2;
    it->clip[3] = h / 2;
  }
}

static void put_all(BKNode *canvas, const Scene *s)
{
  for (int i = 0; i < s->n; i++) {
    BK_canvas_put_item(canvas, i, &s->items[i]);
  }
  BK_canvas_truncate(canvas, s->n);
}

/* Copy item j into slot i (the coords and text too, as a host's arena would). */
static void copy_item(Scene *s, int i, int j)
{
  s->items[i] = s->items[j];
  memcpy(s->coords[i], s->coords[j], sizeof(s->coords[i]));
  memcpy(s->text[i], s->text[j], sizeof(s->text[i]));
  s->items[i].coords = s->coords[i];
  if (s->items[i].text) {
    s->items[i].text = s->text[i];
  }
}

static void mutate(Scene *s)
{
  const int i = s->n ? (int)rnd((uint32_t)s->n) : 0;
  switch (rnd(9)) {
    case 0: /* a new item somewhere in the list: those after it shift a slot */
      if (s->n < MAX_ITEMS) {
        for (int k = s->n; k > i; k--) {
          copy_item(s, k, k - 1);
        }
        s->n++;
        random_item(s, i);
      }
      break;
    case 1: /* one goes: those after it shift back */
      if (s->n > 0) {
        for (int k = i; k + 1 < s->n; k++) {
          copy_item(s, k, k + 1);
        }
        s->n--;
      }
      break;
    case 2:
      if (s->n) {
        s->items[i].fill[rnd(3)] = (float)rnd(256) / 255.0f;
      }
      break;
    case 3:
      if (s->n) {
        s->coords[i][0] += (float)rnd(9) - 4.0f;
        s->coords[i][1] += (float)rnd(9) - 4.0f;
      }
      break;
    case 4:
      if (s->n && s->items[i].type == BK_ITEM_TEXT) {
        snprintf(s->text[i], sizeof(s->text[i]), "t%u", (unsigned)rnd(100000));
      }
      break;
    case 5:
      if (s->n) {
        s->items[i].hidden = !s->items[i].hidden;
      }
      break;
    case 6:
      if (s->n && s->items[i].type == BK_ITEM_IMAGE) {
        s->items[i].imbuf = s->items[i].imbuf == g_img[0] ? g_img[1] : g_img[0];
      }
      break;
    case 7:
      if (s->n) {
        s->items[i].clip_on = !s->items[i].clip_on;
        s->items[i].clip[0] = s->coords[i][0];
        s->items[i].clip[1] = s->coords[i][1];
        s->items[i].clip[2] = 10;
        s->items[i].clip[3] = 10;
      }
      break;
    default: /* nothing changes this frame */
      break;
  }
}

static long damaged_area(const BuiRaster *r)
{
  const int(*rects)[4];
  const int n = BUI_raster_damage(r, &rects);
  long a = 0;
  for (int i = 0; i < n; i++) {
    a += (long)rects[i][2] * rects[i][3];
  }
  return a;
}

/* A fresh canvas with the same items, drawn whole. */
static void reference(const Scene *s, uint32_t *out)
{
  BKNode *canvas = BK_node_new("canvas");
  BKContext *ctx = BK_context_new(canvas);
  BuiRaster *r = BUI_raster_create(W, H);
  put_all(canvas, s);
  BK_render(ctx, r, W, H, 1);
  memcpy(out, BUI_raster_pixels(r), sizeof(uint32_t) * W * H);
  BUI_raster_free(r);
  BK_context_free(ctx);
}

static int first_diff(const uint32_t *a, const uint32_t *b)
{
  for (int i = 0; i < W * H; i++) {
    if (a[i] != b[i]) {
      return i;
    }
  }
  return -1;
}

static void make_images(void)
{
  unsigned char px[2][8 * 8 * 4];
  for (int i = 0; i < 8 * 8; i++) {
    const unsigned char a[4] = {255, (unsigned char)(i * 4), 0, 255}, b[4] = {0, 80, (unsigned char)(i * 4), 255};
    memcpy(&px[0][i * 4], a, 4);
    memcpy(&px[1][i * 4], b, 4);
  }
  g_img[0] = BK_imbuf_from_rgba(px[0], 8, 8, 8 * 4);
  g_img[1] = BK_imbuf_from_rgba(px[1], 8, 8, 8 * 4);
}

static void test_random_frames(void)
{
  static Scene s;
  static uint32_t want[W * H];
  BKNode *canvas = BK_node_new("canvas");
  BKContext *ctx = BK_context_new(canvas);
  BuiRaster *r = BUI_raster_create(W, H);
  s.n = 20;
  for (int i = 0; i < s.n; i++) {
    random_item(&s, i);
  }
  put_all(canvas, &s);
  BK_render(ctx, r, W, H, 1);
  int stale = 0;
  for (int frame = 0; frame < 400 && !stale; frame++) {
    for (int m = (int)rnd(3); m >= 0; m--) {
      mutate(&s);
    }
    put_all(canvas, &s);
    BK_render(ctx, r, W, H, 0);
    reference(&s, want);
    const int at = first_diff(BUI_raster_pixels(r), want);
    if (at >= 0) {
      fprintf(stderr, "frame %d: stale pixel at %d,%d: %08x, a full redraw has %08x\n", frame, at % W, at / W,
              BUI_raster_pixels(r)[at], want[at]);
    }
    stale = at >= 0;
    CHECK(!stale);
  }
  BUI_raster_free(r);
  BK_context_free(ctx);
}

static void test_only_changes_redraw(void)
{
  static Scene s;
  BKNode *canvas = BK_node_new("canvas");
  BKContext *ctx = BK_context_new(canvas);
  BuiRaster *r = BUI_raster_create(W, H);
  s.n = 30;
  for (int i = 0; i < s.n; i++) {
    random_item(&s, i);
  }
  put_all(canvas, &s);
  BK_render(ctx, r, W, H, 1);
  put_all(canvas, &s); /* the first frame laid the canvas out: it settles on the next */
  BK_render(ctx, r, W, H, 0);

  /* the same items again, from a host's fresh buffers: nothing to redraw */
  static Scene again;
  again = s;
  for (int i = 0; i < s.n; i++) {
    copy_item(&again, i, i);
  }
  put_all(canvas, &again);
  CHECK(BK_render(ctx, r, W, H, 0) == 0);

  /* one small rect changes colour: only around it */
  BKCanvasItem dot;
  memset(&dot, 0, sizeof(dot));
  const float box[4] = {10, 10, 22, 22};
  dot.type = BK_ITEM_RECT;
  dot.coords = box;
  dot.ncoords = 4;
  dot.fill[0] = dot.fill[3] = 1.0f;
  again.items[again.n++] = dot;
  put_all(canvas, &again);
  BK_render(ctx, r, W, H, 0);
  again.items[again.n - 1].fill[1] = 1.0f;
  put_all(canvas, &again);
  CHECK(BK_render(ctx, r, W, H, 0) > 0);
  const long area = damaged_area(r);
  CHECK(area > 0 && area < W * H / 20); /* the raster damages whole tiles */

  /* an item leaves: where it was redraws */
  again.n--;
  put_all(canvas, &again);
  CHECK(BK_render(ctx, r, W, H, 0) > 0);
  CHECK(damaged_area(r) < W * H / 20);
  BUI_raster_free(r);
  BK_context_free(ctx);
}

static void place(BKNode *n, int x, int y, int w, int h, int visible)
{
  BK_geom_set(n, "x", x);
  BK_geom_set(n, "y", y);
  BK_geom_set(n, "width", w);
  BK_geom_set(n, "height", h);
  BK_set_num(n, "visible", visible);
}

static void test_placing_again_redraws_nothing(void)
{
  BKNode *canvas = BK_node_new("canvas");
  BKContext *ctx = BK_context_new(canvas);
  BuiRaster *r = BUI_raster_create(W, H);
  BKNode *field = BK_node_new("text");
  BK_node_add(canvas, field, -1);
  BK_geom_set_manager(field, BK_GEOM_PLACE);
  place(field, 20, 20, 120, 24, 1);
  BK_render(ctx, r, W, H, 1);
  BK_render(ctx, r, W, H, 0);

  place(field, 20, 20, 120, 24, 1);
  CHECK(BK_render(ctx, r, W, H, 0) == 0);

  place(field, 30, 20, 120, 24, 1);
  CHECK(BK_render(ctx, r, W, H, 0) > 0);

  place(field, 30, 20, 120, 24, 0);
  CHECK(BK_render(ctx, r, W, H, 0) > 0);
  place(field, 30, 20, 120, 24, 0);
  CHECK(BK_render(ctx, r, W, H, 0) == 0);

  place(field, 30, 20, 120, 24, 1);
  CHECK(BK_render(ctx, r, W, H, 0) > 0);
  BUI_raster_free(r);
  BK_context_free(ctx);
}

int main(void)
{
  make_images();
  test_only_changes_redraw();
  test_placing_again_redraws_nothing();
  test_random_frames();
  BK_imbuf_free(g_img[0]);
  BK_imbuf_free(g_img[1]);
  if (!g_fail) {
    printf("damage_test: ok\n");
  }
  return g_fail;
}
