export interface PaginationProps {
  currentPage: number
  perPage: number
  total: number
  onPageChange: (page: number) => void
}
