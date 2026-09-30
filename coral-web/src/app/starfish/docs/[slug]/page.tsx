import type { Metadata } from "next";
import { notFound } from "next/navigation";
import { DocPageView } from "@/components/starfish/DocPage";
import { docPage } from "@/lib/starfish/docs";

type Props = { params: Promise<{ slug: string }> };

export async function generateMetadata({ params }: Props): Promise<Metadata> {
  const page = docPage((await params).slug);
  return page ? { title: `${page.title} API`, description: page.summary } : {};
}

export default async function DocsCategoryPage({ params }: Props) {
  const page = docPage((await params).slug);
  if (!page) notFound();

  return <DocPageView page={page} />;
}
