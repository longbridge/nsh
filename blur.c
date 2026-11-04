// Backdrop blur cost on GSK: offscreen, nothing shows on any desktop.
#include <gtk/gtk.h>
#include <sys/resource.h>
#include <time.h>
#include <unistd.h>
#define W 1200
#define H 800
#define BAR 240
static double now(void){struct timespec t;clock_gettime(CLOCK_MONOTONIC,&t);return t.tv_sec+t.tv_nsec/1e9;}
static double cpu(void){struct rusage r;getrusage(RUSAGE_SELF,&r);return r.ru_utime.tv_sec+r.ru_utime.tv_usec/1e6+r.ru_stime.tv_sec+r.ru_stime.tv_usec/1e6;}
static void scene(GtkSnapshot*s,int f){
  GdkRGBA bg={.9,.9,.92,1},a={.15,.39,.92,1},b={.96,.62,.04,1};
  graphene_rect_t r=GRAPHENE_RECT_INIT(0,0,W,H);gtk_snapshot_append_color(s,&bg,&r);
  for(int i=0;i<60;i++){graphene_rect_t q=GRAPHENE_RECT_INIT(((i*43+f*3)%(W+80))-80,(i*37)%H,70,28);gtk_snapshot_append_color(s,i%2?&a:&b,&q);}
}
static void layer(GtkSnapshot*s,float y,float h,float sigma,float t0,float t1){
  graphene_rect_t r=GRAPHENE_RECT_INIT(0,y,W,h);
  GskColorStop st[2]={{t0,{0,0,0,0}},{t1,{0,0,0,1}}};
  graphene_point_t p0=GRAPHENE_POINT_INIT(0,y),p1=GRAPHENE_POINT_INIT(0,y+h);
  gtk_snapshot_push_copy(s);
  gtk_snapshot_push_mask(s,GSK_MASK_MODE_ALPHA);
  gtk_snapshot_append_linear_gradient(s,&r,&p0,&p1,st,2);
  gtk_snapshot_pop(s);
  gtk_snapshot_push_clip(s,&r);
  gtk_snapshot_push_blur(s,sigma*2);
  graphene_rect_t big=GRAPHENE_RECT_INIT(0,0,W,H);
  gtk_snapshot_append_paste(s,&big,0);
  gtk_snapshot_pop(s);gtk_snapshot_pop(s);
  gtk_snapshot_pop(s);
  gtk_snapshot_pop(s);
}
static void corners(GtkSnapshot*s,int continuous){
  GdkRGBA c={.1,.1,.15,1};
  for(int i=0;i<100;i++){float x=(i%10)*118+4,y=(i/10)*78+4;
    graphene_rect_t r=GRAPHENE_RECT_INIT(x,y,110,70);
    if(continuous){GskPathBuilder*b=gsk_path_builder_new();float k=28*1.6f;
      gsk_path_builder_move_to(b,x+k,y);gsk_path_builder_line_to(b,x+110-k,y);
      gsk_path_builder_cubic_to(b,x+110-k*.4f,y,x+110,y+k*.4f,x+110,y+k);gsk_path_builder_line_to(b,x+110,y+70-k);
      gsk_path_builder_cubic_to(b,x+110,y+70-k*.4f,x+110-k*.4f,y+70,x+110-k,y+70);gsk_path_builder_line_to(b,x+k,y+70);
      gsk_path_builder_cubic_to(b,x+k*.4f,y+70,x,y+70-k*.4f,x,y+70-k);gsk_path_builder_line_to(b,x,y+k);
      gsk_path_builder_cubic_to(b,x,y+k*.4f,x+k*.4f,y,x+k,y);gsk_path_builder_close(b);
      GskPath*p=gsk_path_builder_free_to_path(b);gtk_snapshot_append_fill(s,p,GSK_FILL_RULE_WINDING,&c);gsk_path_unref(p);
    } else {GskRoundedRect rr;gsk_rounded_rect_init_from_rect(&rr,&r,28);gtk_snapshot_push_rounded_clip(s,&rr);gtk_snapshot_append_color(s,&c,&r);gtk_snapshot_pop(s);}
  }
}
static GskRenderNode*frame(const char*m,int f){
  GtkSnapshot*s=gtk_snapshot_new();
  graphene_point_t o=GRAPHENE_POINT_INIT(0,0);(void)o;gtk_snapshot_scale(s,2,2);
  scene(s,f);
  if(!strcmp(m,"one")) layer(s,H-BAR,BAR,24,0,0.001);
  else if(!strcmp(m,"masked")) layer(s,H-BAR,BAR,24,0,1);
  else if(!strcmp(m,"five")) for(int i=0;i<5;i++){float sg=24.f/(1<<(4-i));layer(s,H-BAR,BAR,sg,i/5.f,(i+1)/5.f);}
  else if(!strcmp(m,"circ")) corners(s,0);
  else if(!strcmp(m,"squircle")) corners(s,1);
  return gtk_snapshot_free_to_node(s);
}
int main(int c,char**v){
  gtk_init();GdkDisplay*d=gdk_display_get_default();const char*kind=v[1];double secs=atof(v[2]);
  GskRenderer*r=!strcmp(kind,"vulkan")?gsk_vulkan_renderer_new():!strcmp(kind,"ngl")?gsk_gl_renderer_new():gsk_cairo_renderer_new();
  GError*e=0;if(!gsk_renderer_realize_for_display(r,d,&e)){printf("%s: no renderer: %s\n",kind,e->message);return 1;}
  graphene_rect_t vp=GRAPHENE_RECT_INIT(0,0,W*2,H*2);guchar*buf=g_malloc(W*2*H*2*4);
  const char*ms[]={"base","one","masked","five","circ","squircle"};
  for(int i=0;i<6;i++){const char*m=ms[i];double t0=now(),c0=cpu(),work=0;int f=0;double next=t0;
    while(now()-t0<secs){double a=now();GskRenderNode*n=frame(m,f);GdkTexture*t=gsk_renderer_render_texture(r,n,&vp);
      gdk_texture_download(t,buf,W*2*4);
      if(f==3&&c>3){char*p=g_strdup_printf("%s/%s-%s.png",v[3],kind,m);gdk_texture_save_to_png(t,p);g_free(p);}
      g_object_unref(t);gsk_render_node_unref(n);work+=now()-a;f++;next+=1/60.;double w=next-now();if(w>0)g_usleep(w*1e6);}
    double wall=now()-t0;printf("%-7s %-9s fps=%5.1f frame=%6.2fms cpu=%6.1f%%\n",kind,m,f/wall,work/f*1e3,(cpu()-c0)/wall*100);fflush(stdout);}
  gsk_renderer_unrealize(r);return 0;}
