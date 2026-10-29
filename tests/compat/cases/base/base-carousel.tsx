// A case of the API's test suite (docs/dev/compatibility.md): `Carousel` of
// `nsh/base` and its parts (`CarouselContent`, `CarouselItem`,
// `CarouselPrevious`, `CarouselNext`, `CarouselPagination`,
// `CarouselPaginationItem`), with no look of their own: the roles, labels
// and states screen readers get, a control or a pagination item reporting
// the next slide (`onChange`) while the app owns the one shown (`index`),
// `looping` around the ends, seamlessly (a runway of a period before the
// slides), the slides laid out by the content's `class` (a gap of 10), the
// drag and focus events the app pauses its own rotation with, and the app's
// look of a pagination item (`selectedStyle`) and of the indicator the core
// places on the item of the slide shown (`indicatorClass`,
// `indicatorStyle`; the view below the first item).
import { Text, View, signal } from "nsh";
import {
  Carousel,
  CarouselContent,
  CarouselItem,
  CarouselNext,
  CarouselPagination,
  CarouselPaginationItem,
  CarouselPrevious,
} from "nsh/base";

function App() {
  const index = signal(0);
  const changes = signal(0);
  const events = signal<string[]>([]);
  const log = (e: string) => events.set([...events(), e]);
  const slide = (i: number, name: string) => (
    <CarouselItem testID={`slide-${i}`} accessibilityLabel={`${i + 1} of 3`} class="h-full w-[200px]">
      <Text>{name}</Text>
    </CarouselItem>
  );
  const dot = (i: number) => (
    <CarouselPaginationItem
      testID={`dot-${i}`}
      index={i}
      accessibilityLabel={`Slide ${i + 1}`}
      class="h-[10px] w-[10px]"
      selectedStyle={{ backgroundColor: "#00ff00" }}
    />
  );
  return (
    <View style={{ flex: 1, paddingTop: 20, gap: 4 }}>
      <Carousel
        testID="carousel"
        index={index()}
        looping
        onDragStart={() => log("dragStart")}
        onDragEnd={() => log("dragEnd")}
        onFocus={() => log("focus")}
        onBlur={() => log("blur")}
        accessibilityLabel="Colors"
        class="gap-[4px]"
        onChange={({ index: next }) => {
          changes.set(changes() + 1);
          index.set(next);
        }}
      >
        <CarouselContent testID="content" class="h-[100px] w-[200px] gap-[10px]">
          {slide(0, "Red")}
          {slide(1, "Green")}
          {slide(2, "Blue")}
        </CarouselContent>
        <CarouselPrevious testID="previous" accessibilityLabel="Previous slide" class="h-[20px]" />
        <CarouselNext testID="next" accessibilityLabel="Next slide" class="h-[20px]" />
        <CarouselPagination
          testID="dots"
          accessibilityLabel="Choose a slide"
          class="flex-row"
          indicatorClass="top-0 bottom-0"
          indicatorStyle={{ backgroundColor: "#ff0000" }}
        >
          {dot(0)}
          {dot(1)}
          {dot(2)}
        </CarouselPagination>
      </Carousel>
      <Text>{`Index: ${index()}`}</Text>
      <Text>{`Changes: ${changes()}`}</Text>
      <Text>{`Events: ${events().join(",")}`}</Text>
    </View>
  );
}

export default App;
